import copy
import unittest
from pathlib import Path
from unittest import mock

import verify_advisory_exceptions as policy


class AdvisoryPolicyTests(unittest.TestCase):
    def test_extracts_space_and_equals_ignore_forms(self):
        text = "cargo audit --ignore RUSTSEC-2026-0194 --ignore=RUSTSEC-2026-0195"

        self.assertEqual(
            policy.extract_audit_ignore_ids(text),
            ["RUSTSEC-2026-0194", "RUSTSEC-2026-0195"],
        )

    def test_exact_ids_reject_extra_missing_and_duplicate_entries(self):
        expected = list(policy.EXPECTED_ADVISORY_IGNORES)
        policy.assert_exact_ids(expected, "fixture")

        invalid_lists = (
            expected + ["RUSTSEC-2099-9999"],
            expected[:-1],
            expected + [expected[-1]],
        )
        for actual in invalid_lists:
            with self.subTest(actual=actual), self.assertRaises(policy.PolicyError):
                policy.assert_exact_ids(actual, "fixture")

    def test_workflow_rejects_equals_form_extra_ignore(self):
        workflow = """
        run: python3 .github/scripts/verify_advisory_exceptions.py check
        run: python3 .github/scripts/verify_advisory_exceptions.py audit
        """
        policy.validate_workflow_text(workflow)

        with self.assertRaises(policy.PolicyError):
            policy.validate_workflow_text(
                workflow + "\nrun: cargo audit --ignore=RUSTSEC-2099-9999\n"
            )

    def test_audit_command_uses_each_canonical_ignore_once(self):
        command = policy.build_audit_command("/workspace/Cargo.lock")
        expected = [
            "cargo",
            "audit",
            "--file",
            "/workspace/Cargo.lock",
            "--deny",
            "warnings",
        ]
        for identifier in policy.EXPECTED_ADVISORY_IGNORES:
            expected.extend(("--ignore", identifier))

        self.assertEqual(command, expected)
        policy.assert_exact_ids(
            policy.extract_audit_ignore_ids(" ".join(command)),
            "generated cargo-audit command",
        )

    def test_exception_fails_on_expiry_date(self):
        with self.assertRaisesRegex(policy.PolicyError, "expired"):
            policy.validate_policy(Path("."), today=policy.EXCEPTION_EXPIRES)

    def test_audit_runs_outside_project_and_user_cargo_configuration(self):
        repo_root = Path("/workspace")
        with mock.patch.object(policy, "validate_policy") as validate, mock.patch.object(
            policy.subprocess, "run"
        ) as run:
            policy.run_audit(repo_root)

        validate.assert_called_once_with(repo_root)
        command = run.call_args.args[0]
        options = run.call_args.kwargs
        self.assertEqual(command, policy.build_audit_command(repo_root / "Cargo.lock"))
        self.assertNotEqual(Path(options["cwd"]), repo_root)
        self.assertEqual(Path(options["env"]["CARGO_HOME"]), Path(options["cwd"]))
        self.assertTrue(options["check"])

    def test_metadata_command_is_locked_and_all_features(self):
        command = policy.metadata_command()

        self.assertIn("--locked", command)
        self.assertIn("--all-features", command)

    def test_optional_target_build_dependency_is_rejected_from_declarations(self):
        metadata = {
            "workspace_members": ["workspace-app"],
            "packages": [
                {
                    "id": "workspace-app",
                    "name": "workspace-app",
                    "version": "1.0.0",
                    "source": None,
                    "dependencies": [
                        {
                            "name": "quick-xml",
                            "source": "registry+https://github.com/rust-lang/crates.io-index",
                            "req": "^0.41",
                            "kind": "build",
                            "rename": None,
                            "optional": True,
                            "uses_default_features": True,
                            "features": [],
                            "target": 'cfg(target_os = "linux")',
                            "registry": None,
                        }
                    ],
                    "targets": [],
                }
            ],
            "resolve": {"nodes": [{"id": "workspace-app", "deps": []}]},
        }

        violations = policy.find_forbidden_workspace_declarations(metadata)

        self.assertEqual(len(violations), 1)
        self.assertIn("optional=True", violations[0])
        self.assertIn("kind=build", violations[0])
        self.assertIn("target=cfg(target_os", violations[0])

    def test_unexpected_full_ancestor_path_is_rejected(self):
        metadata = self._minimal_metadata()
        expected = self._minimal_expected_graph()
        policy.assert_expected_ancestor_edges(metadata, expected)

        changed = copy.deepcopy(metadata)
        changed["packages"].append(
            self._package("runtime-consumer", "runtime-consumer", "1.0.0")
        )
        changed["resolve"]["nodes"][0]["deps"].append(
            self._dependency("runtime-consumer")
        )
        changed["resolve"]["nodes"].append(
            {
                "id": "runtime-consumer",
                "deps": [self._dependency("quick-xml-0.30.0")],
            }
        )

        with self.assertRaises(policy.PolicyError):
            policy.assert_expected_ancestor_edges(changed, expected)

    def test_quick_xml_boundary_prereleases_remain_vulnerable(self):
        expected = self._minimal_expected_graph()
        for version in (
            "0.41.0-alpha.1",
            "0.41.0-beta.2",
            "0.41.0-rc.1+vendor.01",
        ):
            with self.subTest(version=version), self.assertRaisesRegex(
                policy.PolicyError, "vulnerable quick-xml versions changed"
            ):
                policy.assert_expected_ancestor_edges(
                    self._metadata_with_extra_quick_xml(version), expected
                )

    def test_semver_precedence_and_build_metadata(self):
        ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ]

        self.assertEqual(sorted(ordered, key=policy._semver_key), ordered)
        self.assertEqual(
            policy._semver_key("1.0.0+build.1"),
            policy._semver_key("1.0.0+build.2"),
        )

    def test_quick_xml_final_boundary_and_greater_versions_are_patched(self):
        expected = self._minimal_expected_graph()
        for version in (
            "0.41.0",
            "0.41.0+vendor.01",
            "0.41.1-alpha.1",
            "1.0.0-rc.1",
        ):
            with self.subTest(version=version):
                policy.assert_expected_ancestor_edges(
                    self._metadata_with_extra_quick_xml(version), expected
                )

    def test_malformed_cargo_package_versions_fail_closed(self):
        expected = self._minimal_expected_graph()
        for version in (
            "0.41",
            "0.41.0.1",
            "00.41.0",
            "0.41.0-",
            "0.41.0-alpha..1",
            "0.41.0-01",
            "0.41.0-alpha_1",
            "0.41.0+build..1",
            "18446744073709551616.0.0",
        ):
            with self.subTest(version=version), self.assertRaises(policy.PolicyError):
                policy.assert_expected_ancestor_edges(
                    self._metadata_with_extra_quick_xml(version), expected
                )

    @staticmethod
    def _minimal_expected_graph():
        return {
            "0.30.0": {
                ("workspace:workspace-app", "compile-macro@1.0.0"),
                ("compile-macro@1.0.0", "quick-xml@0.30.0"),
            }
        }

    @classmethod
    def _metadata_with_extra_quick_xml(cls, version):
        metadata = cls._minimal_metadata()
        package_id = f"quick-xml-{version}"
        metadata["packages"].append(
            cls._package(package_id, "quick-xml", version)
        )
        metadata["resolve"]["nodes"][0]["deps"].append(
            cls._dependency(package_id)
        )
        metadata["resolve"]["nodes"].append({"id": package_id, "deps": []})
        return metadata

    @classmethod
    def _minimal_metadata(cls):
        return {
            "workspace_members": ["workspace-app"],
            "packages": [
                cls._package("workspace-app", "workspace-app", "1.0.0", source=None),
                cls._package("compile-macro", "compile-macro", "1.0.0"),
                cls._package("quick-xml-0.30.0", "quick-xml", "0.30.0"),
            ],
            "resolve": {
                "nodes": [
                    {
                        "id": "workspace-app",
                        "deps": [cls._dependency("compile-macro")],
                    },
                    {
                        "id": "compile-macro",
                        "deps": [cls._dependency("quick-xml-0.30.0")],
                    },
                    {"id": "quick-xml-0.30.0", "deps": []},
                ]
            },
        }

    @staticmethod
    def _package(
        package_id,
        name,
        version,
        source: str | None = "registry+https://github.com/rust-lang/crates.io-index",
    ):
        return {
            "id": package_id,
            "name": name,
            "version": version,
            "source": source,
            "dependencies": [],
            "targets": [],
        }

    @staticmethod
    def _dependency(package_id):
        return {
            "name": package_id,
            "pkg": package_id,
            "dep_kinds": [{"kind": None, "target": None}],
        }


if __name__ == "__main__":
    unittest.main()

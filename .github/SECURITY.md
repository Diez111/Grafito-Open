# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 1.1.x   | :white_check_mark: |
| 1.0.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

We take the security of Grafito seriously. If you believe you have found a security vulnerability, please report it to us as described below.

**Please do NOT report security vulnerabilities through public GitHub issues.**

### How to Report

Email us at [diezprocapoxd@gmail.com](mailto:diezprocapoxd@gmail.com) with the following information:

- Description of the vulnerability
- Steps to reproduce the issue
- Potential impact
- Any suggested fixes (if applicable)

### What to Expect

- **Acknowledgment**: We will acknowledge receipt of your vulnerability report within 48 hours.
- **Assessment**: We will assess the vulnerability and determine its impact within 7 days.
- **Resolution**: We will work on a fix and release a patched version as soon as possible.
- **Disclosure**: We will coordinate with you on the disclosure timeline.

### Security Measures

- All commits to `main` require GPG signature verification
- All pull requests require at least 1 review approval
- CI/CD pipelines run CodeQL security analysis on every PR
- Dependencies are automatically scanned for known vulnerabilities
- Branch protection prevents force pushes and unauthorized deletions

#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Goldens F2/F3 con ZIP a mano (PK + CRC32 propio) + fuzz-friendly y presupuestos.
use crate::{import_ggb_bytes, GGB_XML_NAME, MAX_DATA_TABLE_ROWS, MAX_ELEMS, MAX_GGB_XML_BYTES};
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}
fn build_zip_store(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::new();
    let mut sizes: Vec<(u32, u32)> = Vec::new();
    for (name, data) in files {
        offsets.push(u32::try_from(out.len()).unwrap_or(0));
        let crc = crc32_ieee(data);
        let size = u32::try_from(data.len()).unwrap_or(0);
        sizes.push((crc, size));
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        let name_bytes = name.as_bytes();
        out.extend_from_slice(&(u16::try_from(name_bytes.len()).unwrap_or(0)).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);
    }
    let cd_start = u32::try_from(out.len()).unwrap_or(0);
    for (idx, (name, _)) in files.iter().enumerate() {
        let (crc, size) = sizes[idx];
        let offset = offsets[idx];
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        let name_bytes = name.as_bytes();
        out.extend_from_slice(&(u16::try_from(name_bytes.len()).unwrap_or(0)).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name_bytes);
    }
    let cd_size = u32::try_from(out.len())
        .unwrap_or(0)
        .saturating_sub(cd_start);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(u16::try_from(files.len()).unwrap_or(0)).to_le_bytes());
    out.extend_from_slice(&(u16::try_from(files.len()).unwrap_or(0)).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
fn ggb_with_xml(xml: &str) -> Vec<u8> {
    build_zip_store(&[(GGB_XML_NAME, xml.as_bytes())])
}
fn ggb_with_xml_and_csv(xml: &str, csv_name: &str, csv: &str) -> Vec<u8> {
    build_zip_store(&[(GGB_XML_NAME, xml.as_bytes()), (csv_name, csv.as_bytes())])
}
fn xml_header() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?><geogebra format="5.0"><construction>"#.to_string()
}
fn xml_footer() -> String {
    "</construction></geogebra>".to_string()
}
fn point_xml(label: &str, x: f64, y: f64) -> String {
    format!(
        r#"<element type="point" label="{label}"><coords x="{x}" y="{y}" z="1" w="1"/></element>"#
    )
}
fn conic_ellipse_xml(label: &str, h: f64, k: f64, rx: f64, ry: f64) -> String {
    let a = 1.0 / (rx * rx);
    let c = 1.0 / (ry * ry);
    let d = -2.0 * h / (rx * rx);
    let e = -2.0 * k / (ry * ry);
    let f = h * h / (rx * rx) + k * k / (ry * ry) - 1.0;
    format!(
        r#"<element type="conic" label="{label}"><matrix A0="{a}" A1="0" A2="{c}" A3="{d}" A4="{e}" A5="{f}"/><eigenvectors x0="1" y0="0" x1="0" y1="1"/></element>"#
    )
}
fn conic_parabola_xml(label: &str, h: f64, k: f64, p: f64) -> String {
    let a = 1.0;
    let d = -2.0 * h;
    let e = -4.0 * p;
    let f = h * h + 4.0 * p * k;
    format!(
        r#"<element type="conic" label="{label}"><matrix A0="{a}" A1="0" A2="0" A3="{d}" A4="{e}" A5="{f}"/><eigenvectors x0="1" y0="0" x1="0" y1="1"/></element>"#
    )
}
fn conic_hyperbola_xml(label: &str, h: f64, k: f64, a_: f64, b_: f64) -> String {
    let a2 = a_ * a_;
    let b2 = b_ * b_;
    let a = 1.0 / a2;
    let c = -1.0 / b2;
    let d = -2.0 * h / a2;
    let e = 2.0 * k / b2;
    let f = h * h / a2 - k * k / b2 - 1.0;
    format!(
        r#"<element type="conic" label="{label}"><matrix A0="{a}" A1="0" A2="{c}" A3="{d}" A4="{e}" A5="{f}"/><eigenvectors x0="1" y0="0" x1="0" y1="1"/></element>"#
    )
}
#[test]
fn golden_conic_ellipse_maps_to_ellipse() {
    let xml = format!(
        "{}{}{}",
        xml_header(),
        conic_ellipse_xml("c", 1.0, 2.0, 3.0, 2.0),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("ellipse golden debe importar");
    assert!(
        rep.tipos.contains_key("Ellipse"),
        "esperaba Ellipse, got {:?}",
        rep.tipos
    );
    let cmd = rep
        .objetos
        .iter()
        .find(|o| o.tipo == "Ellipse")
        .expect("Ellipse objeto")
        .comando
        .clone();
    assert!(
        cmd.starts_with("Ellipse["),
        "comando ellipse mal formado {cmd}"
    );
}
#[test]
fn golden_conic_parabola_maps_to_parabola() {
    let xml = format!(
        "{}{}{}",
        xml_header(),
        conic_parabola_xml("p", 0.0, 0.0, 1.0),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("parabola golden");
    assert!(
        rep.tipos.contains_key("Parabola"),
        "esperaba Parabola {:?}",
        rep.tipos
    );
}
#[test]
fn golden_conic_hyperbola_maps_to_hyperbola() {
    let xml = format!(
        "{}{}{}",
        xml_header(),
        conic_hyperbola_xml("h", 0.0, 0.0, 2.0, 1.5),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("hyperbola golden");
    assert!(
        rep.tipos.contains_key("Hyperbola"),
        "esperaba Hyperbola {:?}",
        rep.tipos
    );
}
#[test]
fn golden_conic_rotated_is_omitted_honest() {
    let xml = format!(
        r#"{}<element type="conic" label="c_rot"><matrix A0="1" A1="0.5" A2="1" A3="0" A4="0" A5="-1"/><eigenvectors x0="0.707" y0="0.707" x1="-0.707" y1="0.707"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("rotated conic debe importar con omitido");
    assert!(
        !rep.tipos.contains_key("Ellipse")
            && !rep.tipos.contains_key("Hyperbola")
            && !rep.tipos.contains_key("Parabola")
    );
    assert!(
        !rep.omitidos.is_empty(),
        "rotada debe generar omitido honesto"
    );
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.razon.contains("rotada") || o.razon.contains("canónica")),
        "razón debe mencionar canónica {:?}",
        rep.omitidos
    );
}
#[test]
fn golden_polygon_via_command_maps_to_polygon() {
    let xml = format!(
        "{}{}{}{}{}{}{}",
        xml_header(),
        point_xml("A", 0.0, 0.0),
        point_xml("B", 1.0, 0.0),
        point_xml("C", 0.0, 1.0),
        r#"<command name="Polygon"><input a0="A" a1="B" a2="C"/><output a0="poly1"/></command>"#,
        "",
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("polygon golden");
    assert!(
        rep.tipos.contains_key("Polygon"),
        "Polygon esperado {:?}",
        rep
    );
    assert!(rep.tipos.contains_key("Point"), "Points esperados");
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.tipo == "Text" && o.razon.contains("área")),
        "Polygon debe generar Text área omitido honesto"
    );
}
#[test]
fn golden_vector_via_command_maps_to_vector() {
    let xml = format!(
        "{}{}{}{}{}",
        xml_header(),
        point_xml("A", 0.0, 0.0),
        point_xml("B", 2.0, 3.0),
        r#"<command name="Vector"><input a0="A" a1="B"/><output a0="v"/></command>"#,
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("vector golden");
    assert!(
        rep.tipos.contains_key("Vector"),
        "Vector esperado {:?}",
        rep.tipos
    );
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.tipo == "Text" && o.razon.contains("medida vector")),
        "Vector debe generar Text medida omitido"
    );
}
#[test]
fn golden_vector_element_maps_to_vector() {
    let xml = format!(
        r#"{}<element type="vector" label="v"><coords x="1" y="2" z="1" w="1"/><startPoint x="0" y="0"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("vector element golden");
    assert!(
        rep.tipos.contains_key("Vector"),
        "Vector elemento esperado {:?}",
        rep.tipos
    );
}
#[test]
fn golden_angle_via_command_maps_to_polygon_and_text() {
    let xml = format!(
        "{}{}{}{}{}{}{}",
        xml_header(),
        point_xml("A", 1.0, 0.0),
        point_xml("B", 0.0, 0.0),
        point_xml("C", 0.0, 1.0),
        r#"<command name="Angle"><input a0="A" a1="B" a2="C"/><output a0="alpha"/></command>"#,
        "",
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("angle golden");
    assert!(
        rep.tipos.contains_key("Polygon"),
        "Angle debe mapear a Polygon {:?}",
        rep.tipos
    );
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.tipo == "Text" && o.razon.contains("ángulo")),
        "Angle debe generar Text medida omitido"
    );
}
#[test]
fn golden_intersect_line_line_maps_to_point_evaluated() {
    let xml = format!(
        "{}{}{}{}{}{}{}{}{}{}",
        xml_header(),
        point_xml("A", 0.0, 0.0),
        point_xml("B", 1.0, 1.0),
        point_xml("C", 0.0, 1.0),
        point_xml("D", 1.0, 0.0),
        r#"<command name="Segment"><input a0="A" a1="B"/><output a0="s1"/></command>"#,
        r#"<command name="Segment"><input a0="C" a1="D"/><output a0="s2"/></command>"#,
        r#"<command name="Intersect"><input a0="s1" a1="s2"/><output a0="P"/></command>"#,
        "",
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("intersect golden");
    let point_cmds: Vec<_> = rep.objetos.iter().filter(|o| o.tipo == "Point").collect();
    assert!(!point_cmds.is_empty(), "Intersect debe generar Point");
    let has_center = point_cmds.iter().any(|o| o.comando.contains("0.5"));
    assert!(
        has_center,
        "Point evaluado debe contener 0.5, cmds {:?}",
        point_cmds.iter().map(|o| &o.comando).collect::<Vec<_>>()
    );
}
#[test]
fn golden_intersect_non_line_line_is_omitted_honest() {
    let xml = format!(
        "{}{}{}{}{}",
        xml_header(),
        point_xml("A", 0.0, 0.0),
        point_xml("B", 1.0, 1.0),
        r#"<command name="Intersect"><input a0="A" a1="B"/><output a0="P"/></command>"#,
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("intersect no-line-line debe importar con omitido");
    let n_points = rep.objetos.iter().filter(|o| o.tipo == "Point").count();
    assert_eq!(
        n_points,
        2,
        "Intersect punto-punto no debe generar Point extra, cmds {:?}",
        rep.objetos.iter().map(|o| &o.comando).collect::<Vec<_>>()
    );
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.tipo == "Intersect" && o.razon.contains("solo line-line")),
        "Intersect no-line-line debe ser omitido honesto solo line-line {:?}",
        rep.omitidos
    );
}
#[test]
fn golden_table_via_numeric_a1_b1() {
    let xml = format!(
        r#"{}<element type="numeric" label="A1"><value val="1"/></element><element type="numeric" label="B1"><value val="2"/></element><element type="numeric" label="A2"><value val="2"/></element><element type="numeric" label="B2"><value val="4"/></element><element type="numeric" label="A3"><value val="3"/></element><element type="numeric" label="B3"><value val="6"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("tabla A1/B1");
    assert!(
        rep.tipos.contains_key("DataTable"),
        "DataTable esperado {:?}",
        rep.tipos
    );
    assert!(
        rep.tipos.contains_key("ScatterPlot"),
        "ScatterPlot esperado"
    );
}
#[test]
fn golden_table_via_csv_inside_zip() {
    let xml = format!(
        "{}{}{}",
        xml_header(),
        point_xml("A", 0.0, 0.0),
        xml_footer()
    );
    let csv = "x,y\n1,2\n2,4\n3,6\n";
    let bytes = ggb_with_xml_and_csv(&xml, "data.csv", csv);
    let rep = import_ggb_bytes(&bytes).expect("tabla CSV");
    assert!(
        rep.tipos.contains_key("DataTable"),
        "CSV DataTable esperado {:?}",
        rep.tipos
    );
    assert!(rep.tipos.contains_key("ScatterPlot"));
}
#[test]
fn golden_table_csv_with_header_and_tsv() {
    let xml = format!("{}{}{}", xml_header(), xml_footer(), "");
    let tsv = "time\tdistance\n0\t1\n1\t3\n2\t5\n";
    let bytes = ggb_with_xml_and_csv(&xml, "hoja.tsv", tsv);
    let rep = import_ggb_bytes(&bytes).expect("tsv");
    assert!(rep.tipos.contains_key("DataTable"));
}
#[test]
fn golden_table_csv_quoted_with_header_maps() {
    let xml = format!("{}{}{}", xml_header(), xml_footer(), "");
    let csv = "\"x\",\"y\"\n\"1\",\"2\"\n\"2\",\"4\"\n\"3\",\"6\"\n";
    let bytes = ggb_with_xml_and_csv(&xml, "quoted.csv", csv);
    let rep = import_ggb_bytes(&bytes).expect("csv con comillas y header");
    assert!(
        rep.tipos.contains_key("DataTable"),
        "CSV con comillas DataTable esperado {:?}",
        rep.tipos
    );
    assert!(
        rep.tipos.contains_key("ScatterPlot"),
        "CSV con comillas ScatterPlot esperado"
    );
    let dt = rep
        .objetos
        .iter()
        .find(|o| o.tipo == "DataTable")
        .expect("DataTable objeto");
    assert!(
        dt.comando.contains('1') && dt.comando.contains('6'),
        "DataTable debe contener valores, got {}",
        dt.comando
    );
}
#[test]
fn honest_todo_text_generic_is_omitted() {
    let xml = format!(
        r#"{}<element type="text" label="txt1"><caption val="Hola mundo"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let rep = import_ggb_bytes(&bytes).expect("text generic");
    assert!(
        rep.omitidos
            .iter()
            .any(|o| o.tipo == "text" && o.razon.contains("Text genérico")),
        "Text debe ser omitido honesto {:?}",
        rep.omitidos
    );
    assert!(!rep.tipos.contains_key("Text"));
}
#[test]
fn honest_omitted_3d_and_cas_and_scripts() {
    let xml2 = format!(
        r#"{}<element type="point" label="A"><coords x="0" y="0" z="1" w="1"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let js = "console.log('hi')";
    let bytes = build_zip_store(&[
        (GGB_XML_NAME, xml2.as_bytes()),
        ("geogebra.js", js.as_bytes()),
    ]);
    let rep = import_ggb_bytes(&bytes).expect("script");
    assert!(
        rep.omitidos.iter().any(|o| o.tipo == "script"),
        "script debe ser omitido"
    );
    let xml3d = format!(
        r#"{}<element type="point3d" label="P3"><coords x="0" y="0" z="1" w="1"/></element>{}"#,
        xml_header(),
        xml_footer()
    );
    let b3 = ggb_with_xml(&xml3d);
    let r3 = import_ggb_bytes(&b3).expect("3d");
    assert!(
        r3.omitidos.iter().any(|o| o.razon.contains("3D")),
        "3D omitido {:?}",
        r3.omitidos
    );
}
#[test]
fn budget_max_ggb_xml_10mib_rejected() {
    assert_eq!(MAX_GGB_XML_BYTES, 10 * 1024 * 1024);
}
#[test]
fn budget_max_elems_5000_enforced() {
    let mut xml = xml_header();
    for i in 0..(MAX_ELEMS + 10) {
        xml.push_str(&point_xml(&format!("P{i}"), i as f64, i as f64));
    }
    xml.push_str(&xml_footer());
    let bytes = ggb_with_xml(&xml);
    let res = import_ggb_bytes(&bytes);
    assert!(res.is_err(), "debe fallar por LimiteElementos 5000");
    if let Err(e) = res {
        assert!(format!("{e}").contains("5000"), "error debe mencionar 5000");
    }
}
#[test]
fn budget_max_data_table_rows_enforced() {
    let mut xml = xml_header();
    for i in 1..=(MAX_DATA_TABLE_ROWS + 1) {
        xml.push_str(&format!(r#"<element type="numeric" label="A{i}"><value val="{i}"/></element><element type="numeric" label="B{i}"><value val="{i}"/></element>"#));
    }
    xml.push_str(&xml_footer());
    let bytes_many = ggb_with_xml(&xml);
    let res_many = import_ggb_bytes(&bytes_many);
    assert!(res_many.is_err(), "demasiados elems debe fallar");
    let mut csv = "x,y\n".to_string();
    for i in 0..=MAX_DATA_TABLE_ROWS {
        csv.push_str(&format!("{i},{}\n", i + 1));
    }
    let xml2 = format!("{}{}{}", xml_header(), xml_footer(), "");
    let b2 = ggb_with_xml_and_csv(&xml2, "big.csv", &csv);
    let rep = import_ggb_bytes(&b2).expect("big csv debe importar con omitido");
    assert!(
        !rep.tipos.contains_key("DataTable") || rep.omitidos.iter().any(|o| o.tipo == "DataTable"),
        "tabla grande debe ser omitida honesta"
    );
}
#[test]
fn fuzz_arbitrary_bytes_never_panic() {
    let cases: &[&[u8]] = &[
        b"",
        b"PK",
        b"\x00\x01\x02\x03",
        b"not a zip",
        &build_zip_store(&[("geogebra.xml", b"not xml")]),
        &build_zip_store(&[(GGB_XML_NAME, b"<geogebra><construction><element type=\"point\" label=\"A\"><coords x=\"NaN\" y=\"inf\"/></element></construction></geogebra>")]),
        &vec![0u8; 1024],
        &vec![255u8; 2048],
    ];
    for data in cases {
        let res = import_ggb_bytes(data);
        let _ = res.is_ok() || res.is_err();
    }
}
#[test]
fn fuzz_random_zip_headers_no_panic() {
    let mut seed: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        seed = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        seed
    };
    for _ in 0..100 {
        let len = (next() % 512) as usize;
        let mut data = Vec::with_capacity(len);
        for _ in 0..len {
            data.push((next() & 0xFF) as u8);
        }
        let _ = import_ggb_bytes(&data);
    }
}
#[test]
fn fuzz_crc32_manual_matches_known() {
    assert_eq!(crc32_ieee(b""), 0);
    assert_eq!(crc32_ieee(b"123456789"), 0xcbf43926);
    assert_eq!(crc32_ieee(b"hello"), 0x3610a686);
}
#[test]
fn reject_doctype_entity_bomb() {
    let xml = r#"<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM "file:///etc/passwd">]><geogebra><construction></construction></geogebra>"#;
    let bytes = ggb_with_xml(xml);
    let res = import_ggb_bytes(&bytes);
    assert!(res.is_err(), "DOCTYPE debe ser rechazado");
}
#[test]
fn manual_zip_is_readable_by_zip_crate() {
    let xml = format!(
        "{}{}{}",
        xml_header(),
        point_xml("A", 1.0, 2.0),
        xml_footer()
    );
    let bytes = ggb_with_xml(&xml);
    let cursor = std::io::Cursor::new(&bytes);
    let mut za = zip::ZipArchive::new(cursor).expect("manual zip debe ser legible");
    let mut found = false;
    for i in 0..za.len() {
        let f = za.by_index(i).unwrap();
        if f.name() == GGB_XML_NAME {
            found = true;
        }
    }
    assert!(found);
}

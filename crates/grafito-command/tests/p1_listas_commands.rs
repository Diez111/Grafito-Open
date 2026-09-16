#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Frente P1: listas de primera clase + estadística/probabilidad (GeoGebra).
//! Happy path + error honesto por comando (2 tests × 82) + visibilidad en
//! paleta + determinismo del RNG + etiquetas DataTable + huérfanos previos.

use grafito_command::{
    command_registry,
    commands::{process_input, CommandOutcome},
};
use grafito_core::{Document, GeoObject};

fn run(document: &mut Document, command: &str) -> CommandOutcome {
    let mut input = command.to_owned();
    process_input(document, &mut input)
}

fn assert_message_contains(document: &mut Document, command: &str, needles: &[&str]) {
    match run(document, command) {
        CommandOutcome::Message(message) => {
            for needle in needles {
                assert!(
                    message.contains(needle),
                    "{command} → {message} (esperaba '{needle}')"
                );
            }
        }
        CommandOutcome::Ok => panic!("{command} dio Ok, esperaba Message"),
        CommandOutcome::Error(message) => {
            panic!("{command} dio Error: {message}")
        }
    }
}

fn assert_error_contains(document: &mut Document, command: &str, needle: &str) {
    match run(document, command) {
        CommandOutcome::Error(message) => assert!(
            message.contains(needle),
            "{command} → {message} (esperaba error con '{needle}')"
        ),
        other => panic!("{command} debió dar Error, dio: {other:?}"),
    }
}

#[test]
fn p1_element_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Element[{10,20,30}, 2]",
        &["Element[2] = 20"],
    );
}

#[test]
fn p1_element_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Element[{1}, 5]", "fuera de rango");
}

#[test]
fn p1_unique_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Unique[{3,1,2,1}]", &["{1, 2, 3}"]);
}

#[test]
fn p1_unique_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Unique[NoExiste]", "NoExiste");
}

#[test]
fn p1_iterationlist_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "IterationList[2*x, x, 1, 4]",
        &["{2, 4, 8, 16}"],
    );
}

#[test]
fn p1_iterationlist_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "IterationList[x, y, 1, 0]", "fuera de rango");
}

#[test]
fn p1_union_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Union[{3,1}, {2,3}]", &["{1, 2, 3}"]);
}

#[test]
fn p1_union_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "Union[{1}]",
        "cantidad de argumentos inválida",
    );
}

#[test]
fn p1_intersection_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Intersection[{3,1}, {2,3}]", &["{3}"]);
}

#[test]
fn p1_intersection_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "Intersection[{1}]",
        "cantidad de argumentos inválida",
    );
}

#[test]
fn p1_insert_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Insert[{1,3}, 2, 2]", &["{1, 2, 3}"]);
}

#[test]
fn p1_insert_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Insert[{1}, 5, 9]", "fuera de rango");
}

#[test]
fn p1_remove_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Remove[{1,2,3}, 2]", &["{1, 3}"]);
}

#[test]
fn p1_remove_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Remove[{1}, 2]", "fuera de rango");
}

#[test]
fn p1_indexof_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "IndexOf[{5,7,9}, 7]", &["IndexOf = 2"]);
}

#[test]
fn p1_indexof_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "IndexOf[{5}, 6]", "no está");
}

#[test]
fn p1_map_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Map[x^2, {1,2,3}]", &["{1, 4, 9}"]);
}

#[test]
fn p1_map_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Map[1/0, {1}]", "no se pudo evaluar");
}

#[test]
fn p1_shuffle_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Shuffle[{1,2,3,4,5}]", &["{"]);
}

#[test]
fn p1_shuffle_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "Shuffle[{1}, {2}]",
        "cantidad de argumentos inválida",
    );
}

#[test]
fn p1_sample_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Sample[{1,2,3,4}, 2]", &["{"]);
}

#[test]
fn p1_sample_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Sample[{1}, 5]", "fuera de rango");
}

#[test]
fn p1_randomelement_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "RandomElement[{10,20,30}]",
        &["RandomElement = "],
    );
}

#[test]
fn p1_randomelement_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "RandomElement[{}]", "vacía");
}

#[test]
fn p1_randomdiscrete_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "RandomDiscrete[1, 6]",
        &["RandomDiscrete = "],
    );
}

#[test]
fn p1_randomdiscrete_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "RandomDiscrete[6, 1]", "min 6 > max 1");
}

#[test]
fn p1_listmin_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "ListMin[{3,1,2}]", &["ListMin = 1"]);
}

#[test]
fn p1_listmin_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ListMin[{}]", "vacía");
}

#[test]
fn p1_listmax_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "ListMax[{3,1,2}]", &["ListMax = 3"]);
}

#[test]
fn p1_listmax_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ListMax[{}]", "vacía");
}

#[test]
fn p1_sum_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Sum[{1,2,3}]", &["Sum = 6"]);
}

#[test]
fn p1_sum_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Sum[{}]", "vacía");
}

#[test]
fn p1_product_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Product[{2,3,4}]", &["Product = 24"]);
}

#[test]
fn p1_product_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Product[{}]", "vacía");
}

#[test]
fn p1_covariance_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Covariance[{1,2,3}, {2,4,6}]",
        &["Covariance = 2.000000"],
    );
}

#[test]
fn p1_covariance_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Covariance[{1}, {1}]", "no computable");
}

#[test]
fn p1_rsquare_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "RSquare[{1,2,3}, {2,4,6}]",
        &["RSquare = 1.000000"],
    );
}

#[test]
fn p1_rsquare_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "RSquare[{1,1}, {1,2}]", "constante");
}

#[test]
fn p1_spearman_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Spearman[{1,2,3}, {3,2,1}]",
        &["Spearman = -1.000000"],
    );
}

#[test]
fn p1_spearman_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Spearman[{1}, {1}]", "mismo largo");
}

#[test]
fn p1_tiedrank_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "TiedRank[{30,10,20}]", &["{3, 1, 2}"]);
}

#[test]
fn p1_tiedrank_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "TiedRank[{}]", "vacía");
}

#[test]
fn p1_ordinalrank_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "OrdinalRank[{1,2,2,3}]", &["{1, 2, 3, 4}"]);
}

#[test]
fn p1_ordinalrank_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "OrdinalRank[{1}, {2}]",
        "cantidad de argumentos inválida",
    );
}

#[test]
fn p1_mad_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "MAD[{1,1,2,2,4}]", &["MAD = 1.000000"]);
}

#[test]
fn p1_mad_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "MAD[{}]", "vacía");
}

#[test]
fn p1_quartile1_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Quartile1[{1,2,3,4}]",
        &["Quartile1 = 1.750000"],
    );
}

#[test]
fn p1_quartile1_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Quartile1[{}]", "vacía");
}

#[test]
fn p1_quartile3_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Quartile3[{1,2,3,4}]",
        &["Quartile3 = 3.250000"],
    );
}

#[test]
fn p1_quartile3_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Quartile3[{}]", "vacía");
}

#[test]
fn p1_percentile_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Percentile[{1,2,3,4}, 50]",
        &["Percentile[50] = 2.500000"],
    );
}

#[test]
fn p1_percentile_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Percentile[{1}, 101]", "fuera de [0, 100]");
}

#[test]
fn p1_sdx_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "SDX[{2,4,4,4,5,5,7,9}]", &["SDX = 2.000000"]);
}

#[test]
fn p1_sdx_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SDX[{}]", "vacía");
}

#[test]
fn p1_sdy_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "SDY[{1,2}, {4,5,6}]", &["SDY = 0.816497"]);
}

#[test]
fn p1_sdy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SDY[{}]", "vacía");
}

#[test]
fn p1_samplesdx_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "SampleSDX[{2,4,4,4,5,5,7,9}]",
        &["SampleSDX = 2.138090"],
    );
}

#[test]
fn p1_samplesdx_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SampleSDX[{5}]", "≥2 datos");
}

#[test]
fn p1_samplesdy_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "SampleSDY[{1,2,3}]",
        &["SampleSDY = 1.000000"],
    );
}

#[test]
fn p1_samplesdy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SampleSDY[{5}]", "≥2 datos");
}

#[test]
fn p1_meanx_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "MeanX[{2,4}]", &["MeanX = 3.000000"]);
}

#[test]
fn p1_meanx_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "MeanX[{}]", "vacía");
}

#[test]
fn p1_meany_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "MeanY[{1,2}, {4,6}]", &["MeanY = 5.000000"]);
}

#[test]
fn p1_meany_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "MeanY[{}]", "vacía");
}

#[test]
fn p1_sigmaxx_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "SigmaXX[{1,2}]", &["SigmaXX = 5.000000"]);
}

#[test]
fn p1_sigmaxx_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SigmaXX[{1e308, 1e308}]", "no finito");
}

#[test]
fn p1_sigmaxy_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "SigmaXY[{1,2}, {3,4}]",
        &["SigmaXY = 11.000000"],
    );
}

#[test]
fn p1_sigmaxy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SigmaXY[{1}, {1,2}]", "largos distintos");
}

#[test]
fn p1_sigmayy_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "SigmaYY[{1,2}]", &["SigmaYY = 5.000000"]);
}

#[test]
fn p1_sigmayy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SigmaYY[{1e308, 1e308}]", "no finito");
}

#[test]
fn p1_sxx_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Sxx[{1,2,3}]", &["Sxx = 2.000000"]);
}

#[test]
fn p1_sxx_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Sxx[{}]", "vacía");
}

#[test]
fn p1_sxy_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Sxy[{1,2}, {2,4}]", &["Sxy = 1.000000"]);
}

#[test]
fn p1_sxy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Sxy[{1}, {2,3}]", "mismo largo");
}

#[test]
fn p1_syy_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Syy[{1,2,3}]", &["Syy = 2.000000"]);
}

#[test]
fn p1_syy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Syy[{}]", "vacía");
}

#[test]
fn p1_geometricmean_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "GeometricMean[{1,4}]",
        &["GeometricMean = 2.000000"],
    );
}

#[test]
fn p1_geometricmean_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "GeometricMean[{-1}]", "> 0");
}

#[test]
fn p1_harmonicmean_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "HarmonicMean[{1,2,4}]",
        &["HarmonicMean = 1.714286"],
    );
}

#[test]
fn p1_harmonicmean_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "HarmonicMean[{0}]", "≠ 0");
}

#[test]
fn p1_mode_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Mode[{1,2,2,3}]", &["Mode = 2.000000"]);
}

#[test]
fn p1_mode_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Mode[{}]", "vacía");
}

#[test]
fn p1_rootmeansquare_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "RootMeanSquare[{3,4}]",
        &["RootMeanSquare = 3.535534"],
    );
}

#[test]
fn p1_rootmeansquare_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "RootMeanSquare[{}]", "vacía");
}

#[test]
fn p1_sumsquarederrors_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "SumSquaredErrors[{1,2,3}]",
        &["SumSquaredErrors = 2.000000"],
    );
}

#[test]
fn p1_sumsquarederrors_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "SumSquaredErrors[{}]", "vacía");
}

#[test]
fn p1_zmeanestimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZMeanEstimate[{1,2,3}, 1, 0.95]",
        &["ZMeanEstimate = [", "2.000000"],
    );
}

#[test]
fn p1_zmeanestimate_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ZMeanEstimate[{1}, 0, 0.95]", "sigma");
}

#[test]
fn p1_zmean2estimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZMean2Estimate[{1,2}, 1, {3,4}, 1, 0.95]",
        &["ZMean2Estimate = [", "-2.000000"],
    );
}

#[test]
fn p1_zmean2estimate_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "ZMean2Estimate[{1}, 0, {2}, 1, 0.95]",
        "sigmas",
    );
}

#[test]
fn p1_zmeantest_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZMeanTest[{1,2,3}, 2, 1]",
        &["z = 0.000000, p = 1.000000"],
    );
}

#[test]
fn p1_zmeantest_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ZMeanTest[{1}, 0, 0]", "sigma");
}

#[test]
fn p1_zmean2test_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZMean2Test[{1,2}, 1, {1,2}, 1]",
        &["z = 0.000000, p = 1.000000"],
    );
}

#[test]
fn p1_zmean2test_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ZMean2Test[{1}, 0, {1}, 1]", "sigmas");
}

#[test]
fn p1_zproportionestimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZProportionEstimate[5, 10, 0.95]",
        &["0.500000"],
    );
}

#[test]
fn p1_zproportionestimate_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "ZProportionEstimate[11, 10, 0.95]",
        "0 ≤ éxitos ≤ n",
    );
}

#[test]
fn p1_zproportion2estimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZProportion2Estimate[5, 10, 5, 10, 0.95]",
        &["0.000000"],
    );
}

#[test]
fn p1_zproportion2estimate_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "ZProportion2Estimate[5, 0, 5, 10, 0.95]",
        "n ≥ 1",
    );
}

#[test]
fn p1_zproportiontest_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZProportionTest[5, 10, 0.5]",
        &["z = 0.000000, p = 1.000000"],
    );
}

#[test]
fn p1_zproportiontest_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ZProportionTest[5, 10, 0]", "p0");
}

#[test]
fn p1_zproportion2test_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ZProportion2Test[5, 10, 5, 10]",
        &["z = 0.000000, p = 1.000000"],
    );
}

#[test]
fn p1_zproportion2test_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ZProportion2Test[0, 0, 0, 0]", "n ≥ 1");
}

#[test]
fn p1_tmeanestimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "TMeanEstimate[{1,2,3}, 0.95]",
        &["TMeanEstimate = ["],
    );
}

#[test]
fn p1_tmeanestimate_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "TMeanEstimate[{1,2,3}]",
        "cantidad de argumentos inválida",
    );
}

#[test]
fn p1_tmean2estimate_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "TMean2Estimate[{1,2,3}, {1,2,3}, 0.95]",
        &["0.000000"],
    );
}

#[test]
fn p1_tmean2estimate_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "TMean2Estimate[{1}, {2}, 0.95]", "≥2 datos");
}

#[test]
fn p1_contingencytable_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "ContingencyTable[{10,10,10,10}, 2]",
        &["χ² = 0.000000, gl = 1, p = 1.000000"],
    );
}

#[test]
fn p1_contingencytable_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "ContingencyTable[{1,2,3}, 2]", "≥2×2");
}

#[test]
fn p1_class_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Class[{1,2,3,4}, 2, 1]",
        &["Class[1] = [1.000000, 2.500000)"],
    );
}

#[test]
fn p1_class_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Class[{1}, 2, 5]", "fuera de [1, 2]");
}

#[test]
fn p1_classes_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Classes[{1,2,3,4}, 2]",
        &["Classes(k=2", "{1, 2.5, 4}"],
    );
}

#[test]
fn p1_classes_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Classes[{1}, 0]", "fuera de [1, 1024]");
}

#[test]
fn p1_dotplot_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "DotPlot[{1,2,2,3}]", &["DotPlot created"]);
}

#[test]
fn p1_dotplot_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "DotPlot[{}]", "vacía");
}

#[test]
fn p1_frequencypolygon_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "FrequencyPolygon[{1,2,3,4}]",
        &["FrequencyPolygon created"],
    );
}

#[test]
fn p1_frequencypolygon_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "FrequencyPolygon[{}]", "vacía");
}

#[test]
fn p1_erlang_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Erlang[2, 1, 1]",
        &["PDF(1) = 0.367879", "CDF(1) = 0.264241"],
    );
}

#[test]
fn p1_erlang_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Erlang[0, 1, 1]", "k ≥ 1");
}

#[test]
fn p1_fdistribution_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "FDistribution[5, 10, 1]",
        &["PDF(1) = ", "CDF(1) = "],
    );
}

#[test]
fn p1_fdistribution_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "FDistribution[0, 1, 1]", "finitos > 0");
}

#[test]
fn p1_gamma_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Gamma[2, 1, 1]",
        &["PDF(1) = 0.367879", "CDF(1) = 0.264241"],
    );
}

#[test]
fn p1_gamma_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Gamma[-1, 1, 1]", "alpha");
}

#[test]
fn p1_hypergeometric_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "HyperGeometric[10, 4, 3, 1]",
        &["PMF(1) = 0.500000"],
    );
}

#[test]
fn p1_hypergeometric_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "HyperGeometric[5, 6, 1, 0]", "K ≤ N");
}

#[test]
fn p1_lognormal_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "LogNormal[0, 1, 1]",
        &["PDF(1) = 0.398942", "CDF(1) = 0.500000"],
    );
}

#[test]
fn p1_lognormal_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "LogNormal[0, 0, 1]", "sigma > 0");
}

#[test]
fn p1_logistic_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Logistic[0, 1, 0]",
        &["PDF(0) = 0.250000", "CDF(0) = 0.500000"],
    );
}

#[test]
fn p1_logistic_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Logistic[0, -1, 0]", "s > 0");
}

#[test]
fn p1_pascal_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Pascal[2, 0.5, 0]", &["PMF(0) = 0.250000"]);
}

#[test]
fn p1_pascal_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Pascal[0, 0.5, 0]", "r ≥ 1");
}

#[test]
fn p1_triangular_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Triangular[0, 2, 1, 1]",
        &["PDF(1) = 1.000000", "CDF(1) = 0.500000"],
    );
}

#[test]
fn p1_triangular_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Triangular[2, 0, 1, 1]", "a ≤ c ≤ b");
}

#[test]
fn p1_weibull_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "Weibull[1, 1, 1]",
        &["PDF(1) = 0.367879", "CDF(1) = 0.632121"],
    );
}

#[test]
fn p1_weibull_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Weibull[0, 1, 1]", "finitos > 0");
}

#[test]
fn p1_zipf_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Zipf[1, 3, 1]", &["PMF(1) = 0.545455"]);
}

#[test]
fn p1_zipf_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Zipf[0, 3, 1]", "s > 0");
}

#[test]
fn p1_bernoulli_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "Bernoulli[0.3, 1]", &["PMF(1) = 0.300000"]);
}

#[test]
fn p1_bernoulli_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "Bernoulli[2, 1]", "[0,1]");
}

#[test]
fn p1_tdistribution_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "TDistribution[10, 0]",
        &["CDF(0) = 0.500000"],
    );
}

#[test]
fn p1_tdistribution_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "TDistribution[0, 0]", "df finito > 0");
}

#[test]
fn p1_inversebeta_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseBeta[0.5, 2, 2]", &["= 0.500000"]);
}

#[test]
fn p1_inversebeta_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseBeta[1.5, 2, 2]", "(0,1)");
}

#[test]
fn p1_inversebinomial_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseBinomial[0.5, 10, 0.5]", &["= 5"]);
}

#[test]
fn p1_inversebinomial_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseBinomial[0.5, 10, 2]", "[0,1]");
}

#[test]
fn p1_inversebinomialminimumtrials_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "InverseBinomialMinimumTrials[0.9, 1, 0.5]",
        &["= 4"],
    );
}

#[test]
fn p1_inversebinomialminimumtrials_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "InverseBinomialMinimumTrials[0.9, 1, 0]",
        "(0,1)",
    );
}

#[test]
fn p1_inversecauchy_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseCauchy[0.5, 0, 1]", &["= 0.000000"]);
}

#[test]
fn p1_inversecauchy_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseCauchy[0.5, 0, 0]", "gamma > 0");
}

#[test]
fn p1_inversegamma_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseGamma[0.5, 1, 1]", &["= 0.693147"]);
}

#[test]
fn p1_inversegamma_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseGamma[0.5, -1, 1]", "finitos > 0");
}

#[test]
fn p1_inversehypergeometric_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "InverseHyperGeometric[0.5, 10, 4, 3]",
        &["= 1"],
    );
}

#[test]
fn p1_inversehypergeometric_error() {
    let mut document = Document::new();
    assert_error_contains(
        &mut document,
        "InverseHyperGeometric[0.5, 5, 6, 1]",
        "K ≤ N",
    );
}

#[test]
fn p1_inverselognormal_happy() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "InverseLogNormal[0.5, 0, 1]",
        &["= 1.000000"],
    );
}

#[test]
fn p1_inverselognormal_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseLogNormal[0.5, 0, 0]", "sigma > 0");
}

#[test]
fn p1_inverselogistic_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseLogistic[0.5, 2, 1]", &["= 2.000000"]);
}

#[test]
fn p1_inverselogistic_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseLogistic[0.5, 2, 0]", "s > 0");
}

#[test]
fn p1_inversepascal_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InversePascal[0.5, 1, 0.5]", &["= 0"]);
}

#[test]
fn p1_inversepascal_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InversePascal[0.5, 0, 0.5]", "r ≥ 1");
}

#[test]
fn p1_inversepoisson_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InversePoisson[0.5, 1]", &["= 1"]);
}

#[test]
fn p1_inversepoisson_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InversePoisson[0.5, 0]", "lambda finito > 0");
}

#[test]
fn p1_inverseweibull_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseWeibull[0.5, 1, 2]", &["= 1.386294"]);
}

#[test]
fn p1_inverseweibull_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseWeibull[0.5, 0, 2]", "finitos > 0");
}

#[test]
fn p1_inversezipf_happy() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "InverseZipf[0.5, 1, 3]", &["= 1"]);
}

#[test]
fn p1_inversezipf_error() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "InverseZipf[0.5, 0, 3]", "s > 0");
}

#[test]
fn p1_palette_muestra_los_82() {
    for canonical in [
        "Element",
        "Unique",
        "IterationList",
        "Union",
        "Intersection",
        "Insert",
        "Remove",
        "IndexOf",
        "Map",
        "Shuffle",
        "Sample",
        "RandomElement",
        "RandomDiscrete",
        "ListMin",
        "ListMax",
        "Sum",
        "Product",
        "Covariance",
        "RSquare",
        "Spearman",
        "TiedRank",
        "OrdinalRank",
        "MAD",
        "Quartile1",
        "Quartile3",
        "Percentile",
        "SDX",
        "SDY",
        "SampleSDX",
        "SampleSDY",
        "MeanX",
        "MeanY",
        "SigmaXX",
        "SigmaXY",
        "SigmaYY",
        "Sxx",
        "Sxy",
        "Syy",
        "GeometricMean",
        "HarmonicMean",
        "Mode",
        "RootMeanSquare",
        "SumSquaredErrors",
        "ZMeanEstimate",
        "ZMean2Estimate",
        "ZMeanTest",
        "ZMean2Test",
        "ZProportionEstimate",
        "ZProportion2Estimate",
        "ZProportionTest",
        "ZProportion2Test",
        "TMeanEstimate",
        "TMean2Estimate",
        "ContingencyTable",
        "Class",
        "Classes",
        "DotPlot",
        "FrequencyPolygon",
        "Erlang",
        "FDistribution",
        "Gamma",
        "HyperGeometric",
        "LogNormal",
        "Logistic",
        "Pascal",
        "Triangular",
        "Weibull",
        "Zipf",
        "Bernoulli",
        "TDistribution",
        "InverseBeta",
        "InverseBinomial",
        "InverseBinomialMinimumTrials",
        "InverseCauchy",
        "InverseGamma",
        "InverseHyperGeometric",
        "InverseLogNormal",
        "InverseLogistic",
        "InversePascal",
        "InversePoisson",
        "InverseWeibull",
        "InverseZipf",
    ] {
        let spec = command_registry::resolve(canonical)
            .unwrap_or_else(|| panic!("{canonical} registrado"));
        assert!(spec.palette_visible, "{canonical} visible en paleta");
        assert_eq!(spec.canonical, canonical);
    }
}

#[test]
fn p1_rng_determinista_en_documentos_nuevos() {
    for command in [
        "Shuffle[{1,2,3,4,5,6,7,8}]",
        "Sample[{1,2,3,4,5,6,7,8}, 3]",
        "RandomElement[{10,20,30,40}]",
        "RandomDiscrete[1, 100]",
    ] {
        let mut first = Document::new();
        let mut second = Document::new();
        let a = run(&mut first, command);
        let b = run(&mut second, command);
        match (a, b) {
            (CommandOutcome::Message(ma), CommandOutcome::Message(mb)) => {
                assert_eq!(ma, mb, "{command} debe ser determinista");
            }
            other => panic!("{command} debió dar Message: {other:?}"),
        }
    }
}

#[test]
fn p1_listas_resuelven_columna_datatable() {
    let mut document = Document::new();
    match run(&mut document, "DataTable[{1, 2, 3}, {2, 4, 6}]") {
        CommandOutcome::Ok | CommandOutcome::Message(_) => {}
        CommandOutcome::Error(message) => panic!("fixture DataTable: {message}"),
    }
    let label = document
        .objects_iter()
        .find_map(|(_, obj)| match obj {
            GeoObject::DataTable(table) => Some(table.label.clone()),
            _ => None,
        })
        .expect("fixture no creó DataTable");
    assert_message_contains(&mut document, &format!("Sum[{label}.xs]"), &["Sum = 6"]);
    assert_message_contains(
        &mut document,
        &format!("MeanY[{label}.xs, {label}.ys]"),
        &["MeanY = 4.000000"],
    );
}

#[test]
fn p1_dotplot_crea_scatter_con_bastones() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "DotPlot[{1,2,2,3}]", &["DotPlot created"]);
    let found = document.objects_iter().any(|(_, obj)| match obj {
        GeoObject::ScatterPlot(scatter) => {
            scatter.xs.len() == 4 && scatter.ys == vec![1.0, 1.0, 2.0, 1.0]
        }
        _ => false,
    });
    assert!(
        found,
        "DotPlot debe crear ScatterPlot con alturas de ocurrencia"
    );
}

#[test]
fn p1_freq_polygon_crea_polyline_de_puntos_medios() {
    let mut document = Document::new();
    assert_message_contains(
        &mut document,
        "FrequencyPolygon[{1,2,3,4}]",
        &["FrequencyPolygon created"],
    );
    let found = document.objects_iter().any(|(_, obj)| match obj {
        GeoObject::Polyline(poly) => poly.points.len() == 10,
        _ => false,
    });
    assert!(
        found,
        "FrequencyPolygon debe crear Polyline con 10 puntos medios"
    );
}

#[test]
fn p1_huerfanos_previos_siguen_respondiendo() {
    // Descartes honestos: Cauchy y BetaDist ya respondían (brazos huérfanos
    // previos con la misma firma); la ola no los duplica.
    let mut document = Document::new();
    assert_message_contains(&mut document, "Cauchy[0, 1, 0]", &["CDF(0) = 0.500000"]);
    assert_message_contains(&mut document, "BetaDist[2, 2, 0.5]", &["PDF(0.5)"]);
    // Gamma[x] de 1 argumento sigue siendo la función Γ (sobrecarga honesta).
    assert_message_contains(&mut document, "Gamma[1]", &["Γ(1) = 1.000000"]);
}

#[test]
fn p1_alias_espanol_resuelven() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "elemento[{5,6}, 1]", &["Element[1] = 5"]);
    assert_message_contains(&mut document, "muestrear[{1,2,3}, 2]", &["{"]);
    assert_message_contains(&mut document, "inv_poisson[0.5, 1]", &["= 1"]);
}

#[test]
fn p1b_list_constructor_persists_and_resolves() {
    let mut document = Document::new();
    assert_message_contains(&mut document, "List[{1,2,3}]", &["List", "{1, 2, 3}"]);
    let label = document
        .objects_iter()
        .find_map(|(_, o)| matches!(o, GeoObject::List(_)).then(|| o.label().to_string()))
        .expect("lista creada con etiqueta");
    // Referenciable por los comandos de la familia vía etiqueta.
    assert_message_contains(&mut document, &format!("Element[{label},2]"), &["2"]);
    assert_message_contains(&mut document, &format!("Sum[{label}]"), &["6"]);
    // Serializa y vuelve intacta.
    let saved = grafito_core::serialize_document(&document).expect("serializa");
    let back = grafito_core::deserialize_document(&saved).expect("deserializa");
    let found = back
        .objects_iter()
        .any(|(_, o)| matches!(o, GeoObject::List(l) if l.items.len() == 3));
    assert!(found, "la lista persiste el roundtrip");
}

#[test]
fn p1b_list_rejects_nonfinite_and_reports() {
    let mut document = Document::new();
    assert_error_contains(&mut document, "List[{1/0}]", "List");
}

#[test]
fn p1b_list_budgets_match_across_crates() {
    // Las cotas canónicas viven en core::validation; list_ops las espeja
    // (geometry no puede depender de core). Igualdad blindada acá.
    assert_eq!(
        grafito_core::validation::MAX_LIST_LENGTH,
        10_000,
        "cota de longitud"
    );
    assert_eq!(
        grafito_core::validation::MAX_LIST_DEPTH,
        8,
        "cota de anidado"
    );
    assert_eq!(
        grafito_geometry::list_ops::MAX_LIST_LENGTH,
        grafito_core::validation::MAX_LIST_LENGTH
    );
    assert_eq!(
        grafito_geometry::list_ops::MAX_LIST_DEPTH,
        grafito_core::validation::MAX_LIST_DEPTH
    );
}

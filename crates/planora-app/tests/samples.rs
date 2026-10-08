use planora_app::samples::*;
use planora_kernel::*;
use planora_math::*;

#[test]
fn every_extrude_sample_builds_a_valid_solid() {
    let k = AnalyticKernel::default();
    for sample in ExtrudeSample::ALL {
        let solid = k
            .extrude(&sample.profile(), &Frame::WORLD, sample.default_height())
            .unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
        validate(&solid).unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
    }
}

#[test]
fn every_revolve_sample_builds_a_valid_solid() {
    let k = AnalyticKernel::default();
    for sample in RevolveSample::ALL {
        let solid = k
            .revolve(&sample.chain(), &Frame::WORLD)
            .unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
        validate(&solid).unwrap_or_else(|e| panic!("{}: {e}", sample.label()));
    }
}

#[test]
fn sample_labels_are_unique_and_not_empty() {
    let mut labels: Vec<_> = ExtrudeSample::ALL.iter().map(|s| s.label()).collect();
    labels.extend(RevolveSample::ALL.iter().map(|s| s.label()));
    assert!(labels.iter().all(|l| !l.is_empty()));
    let mut sorted = labels.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), labels.len());
}

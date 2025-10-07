mod common;

use rand::{SeedableRng, rngs::StdRng};
use rollatorium::{EvalConfig, Value, eval, eval_with_rng, parse};

use common::r;

// =============================================================================
// Annotation Grammar Integration
// =============================================================================

#[test]
fn test_multiple_annotations_preserve_dice_result() {
    let annotated = parse(&"4d6kh3[str][fire]").expect("annotated expression parses");
    let baseline = parse(&"4d6kh3").expect("baseline expression parses");

    let rng_seed = 0xFEED_BEEF_u64;
    let annotated_result = eval_with_rng(
        &annotated,
        EvalConfig::default(),
        StdRng::seed_from_u64(rng_seed),
    )
    .expect("annotated evaluation succeeds");
    let baseline_result = eval_with_rng(
        &baseline,
        EvalConfig::default(),
        StdRng::seed_from_u64(rng_seed),
    )
    .expect("baseline evaluation succeeds");

    assert!(
        (annotated_result.total - baseline_result.total).abs() < 1e-9,
        "annotations should not change totals"
    );

    match annotated_result.value {
        Value::Annotated { annotations, expr } => {
            assert_eq!(annotations.len(), 2);
            assert_eq!(annotations[0].text, "str");
            assert_eq!(annotations[1].text, "fire");
            let inner = *expr;
            assert!(
                matches!(inner.value, Value::Dice(_)),
                "inner value should be dice"
            );
        }
        other => panic!("expected annotated dice result, got {:?}", other),
    }
}

#[test]
fn test_nested_annotations_structure() {
    let ast = parse(&"((1 + 2)[inner])[outer]").expect("parse nested annotations");
    let result = eval(&ast).expect("evaluate nested annotations");

    assert_eq!(result.total, 3.0);

    match result.value {
        Value::Annotated { annotations, expr } => {
            let texts: Vec<_> = annotations.iter().map(|ann| ann.text.as_str()).collect();
            assert_eq!(texts, ["inner", "outer"]);
            let inner = *expr;
            assert!(
                matches!(inner.value, Value::Binary { .. }),
                "expected binary operation inside annotations"
            );
        }
        other => panic!("expected annotated value, got {:?}", other),
    }
}

#[test]
fn test_annotations_with_set_operations() {
    // Baseline sanity check that expression stays in expected range even with annotations
    for _ in 0..100 {
        let value = r("(1d4, 2, 6)kh2[advantage]");
        assert!(
            (8.0..=10.0).contains(&value),
            "unexpected total from annotated set: {}",
            value
        );
    }
}

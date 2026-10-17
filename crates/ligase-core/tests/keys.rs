//! M3: what a step's cache key is made of.
//!
//! `CLAUDE.md` fixes the recipe: hash(function code, parameters, input
//! content hashes, external tool versions). The tests here are mostly about
//! the two halves of getting that right — everything that changes a result
//! must change the key, and nothing that does not must change it, because a
//! key that moves when it should not is a cache that never hits and a key
//! that stays when it should not is a wrong answer served quickly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::hash::ContentHash;
use ligase::key::Ingredients;
use ligase::types::DataKind;

fn ingredients() -> Ingredients {
    Ingredients {
        step: "call".into(),
        code: "def call(aln, ref, min_freq=0.01): ...".into(),
        recipe: "bcftools mpileup -f {ref} {aln} | bcftools call -mv -Oz -o {out}".into(),
        output: DataKind::Variants,
        parameters: vec![("min_freq".into(), "0.01".into())],
        inputs: vec![
            ("aln".into(), ContentHash::of_bytes(b"an alignment")),
            ("ref".into(), ContentHash::of_bytes(b"a reference")),
        ],
        tools: vec![("bcftools".into(), "1.24".into())],
    }
}

/// The key, with one ingredient changed by `edit`.
fn key_with(edit: impl FnOnce(&mut Ingredients)) -> String {
    let mut changed = ingredients();
    edit(&mut changed);
    changed.key().to_hex()
}

#[test]
fn the_same_ingredients_give_the_same_key() {
    assert_eq!(ingredients().key(), ingredients().key());
}

#[test]
fn changing_the_code_changes_the_key() {
    assert_ne!(
        key_with(|i| i.code = "def call(aln, ref, min_freq=0.01): pass".into()),
        ingredients().key().to_hex()
    );
}

#[test]
fn changing_a_parameter_changes_the_key() {
    // The M3 acceptance test: `min_freq` moves, so `call` reruns.
    assert_ne!(
        key_with(|i| i.parameters = vec![("min_freq".into(), "0.05".into())]),
        ingredients().key().to_hex()
    );
}

#[test]
fn changing_an_input_changes_the_key() {
    assert_ne!(
        key_with(|i| i.inputs[0].1 = ContentHash::of_bytes(b"another alignment")),
        ingredients().key().to_hex()
    );
}

#[test]
fn changing_a_tool_version_changes_the_key() {
    // Two bcftools releases that disagree about a call are the reproducibility
    // failure this whole project is aimed at.
    assert_ne!(
        key_with(|i| i.tools = vec![("bcftools".into(), "1.21".into())]),
        ingredients().key().to_hex()
    );
}

#[test]
fn changing_the_command_changes_the_key() {
    assert_ne!(
        key_with(|i| i.recipe = "bcftools mpileup -f {ref} {aln} -o {out}".into()),
        ingredients().key().to_hex()
    );
}

#[test]
fn changing_what_the_step_produces_changes_the_key() {
    assert_ne!(
        key_with(|i| i.output = DataKind::Alignment),
        ingredients().key().to_hex()
    );
}

#[test]
fn renaming_a_step_changes_the_key() {
    assert_ne!(
        key_with(|i| i.step = "call2".into()),
        ingredients().key().to_hex()
    );
}

#[test]
fn an_input_bound_to_a_different_parameter_changes_the_key() {
    // Same two files, swapped between `aln` and `ref`: a different command
    // entirely, since the template addresses them by name.
    assert_ne!(
        key_with(|i| {
            let (first, second) = (i.inputs[0].1, i.inputs[1].1);
            i.inputs[0].1 = second;
            i.inputs[1].1 = first;
        }),
        ingredients().key().to_hex()
    );
}

#[test]
fn listing_the_same_ingredients_in_another_order_does_not() {
    // Inputs, parameters and tools are bound by name, so the order they
    // happen to be collected in says nothing about the result.
    assert_eq!(
        key_with(|i| {
            i.inputs.reverse();
            i.parameters.reverse();
            i.tools.reverse();
        }),
        ingredients().key().to_hex()
    );
}

#[test]
fn ingredients_cannot_be_confused_with_each_other() {
    // Without domain separation, a parameter named `x` with value `y` and
    // one named `xy` with no value would concatenate identically.
    let run = |parameters: Vec<(String, String)>| {
        key_with(|i| {
            i.inputs.clear();
            i.tools.clear();
            i.parameters = parameters;
        })
    };

    assert_ne!(
        run(vec![("x".into(), "yz".into())]),
        run(vec![("xy".into(), "z".into())])
    );
}

#[test]
fn a_key_is_hex_and_whole() {
    let key = ingredients().key();
    let hex = key.to_hex();

    assert_eq!(hex.len(), 64);
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(key.to_string(), hex);
}

#[test]
fn the_key_format_is_pinned_to_its_version() {
    // A cache entry written by one version of this format must never be read
    // by another: the entry would be a claim about ingredients that were
    // combined differently. Bumping `KEY_VERSION` is how that is made safe,
    // and this test is what makes an accidental change to the composition
    // visible as a failure rather than as a silently cold cache.
    assert_eq!(ligase::key::KEY_VERSION, 1);
    assert_eq!(
        ingredients().key().to_hex(),
        "0715ce71f2b7b7fe943455987b34a372d87f8ed1bc5dfde59133b15168189c2f",
    );
}

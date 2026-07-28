use minco_test::FixtureSequence;

#[derive(Debug, PartialEq, Eq)]
struct OrderFixture {
    id: String,
    customer: &'static str,
}

#[test]
fn fixture_builders_are_repeatable_and_orm_independent() {
    let build = || {
        let mut fixtures = FixtureSequence::new("orders").expect("valid fixture namespace");
        let first = fixtures
            .build("order", |identity| OrderFixture {
                id: identity.stable_id,
                customer: "Ada",
            })
            .expect("build first fixture");
        let second = fixtures
            .build("order", |identity| OrderFixture {
                id: identity.stable_id,
                customer: "Grace",
            })
            .expect("build second fixture");
        (first, second)
    };

    let first_run = build();
    let second_run = build();
    assert_eq!(first_run, second_run);
    assert_eq!(first_run.0.id, "orders:order:00000001");
    assert_eq!(first_run.1.id, "orders:order:00000002");
}

#[test]
fn invalid_fixture_identity_parts_fail_closed_without_advancing_the_sequence() {
    let mut fixtures = FixtureSequence::new("orders").expect("valid fixture namespace");
    let error = fixtures
        .next("Order Item")
        .expect_err("invalid fixture kind");
    assert!(error.to_string().contains("fixture kind"));

    let identity = fixtures.next("order-item").expect("valid fixture kind");
    assert_eq!(identity.ordinal, 1);
    assert_eq!(identity.stable_id, "orders:order-item:00000001");
}

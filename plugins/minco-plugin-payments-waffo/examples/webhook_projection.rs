//! Application-owned webhook projection after provider verification.

use minco_plugin_payments_waffo::VerifiedWaffoWebhook;

pub trait DeliveryClaims {
    fn claim(&mut self, key: &str) -> bool;
}

pub trait OrdersProjection {
    fn mark_paid(&mut self, provider_event_id: &str);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionOutcome {
    Handled,
    Duplicate,
    Ignored,
}

pub fn project_verified(
    verified: &VerifiedWaffoWebhook,
    delivery_claims: &mut impl DeliveryClaims,
    event_claims: &mut impl DeliveryClaims,
    orders: &mut impl OrdersProjection,
) -> ProjectionOutcome {
    if !delivery_claims.claim(&verified.delivery_dedupe_key)
        || !event_claims.claim(&verified.event_dedupe_key)
    {
        return ProjectionOutcome::Duplicate;
    }
    if verified.event.event_type == "order.completed" {
        orders.mark_paid(&verified.event.event_id);
        ProjectionOutcome::Handled
    } else {
        ProjectionOutcome::Ignored
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;
    use minco_plugin_payments_waffo::{WaffoWebhookEvent, WaffoWebhookMode};
    use serde_json::json;
    use std::collections::BTreeSet;

    #[derive(Debug, Default)]
    struct FakeClaims(BTreeSet<String>);

    impl DeliveryClaims for FakeClaims {
        fn claim(&mut self, key: &str) -> bool {
            self.0.insert(key.to_owned())
        }
    }

    #[derive(Debug, Default)]
    struct FakeOrders(Vec<String>);

    impl OrdersProjection for FakeOrders {
        fn mark_paid(&mut self, provider_event_id: &str) {
            self.0.push(provider_event_id.to_owned());
        }
    }

    #[test]
    fn application_owns_projection_and_duplicate_policy() {
        let verified = VerifiedWaffoWebhook {
            timestamp_milliseconds: 1,
            delivery_dedupe_key: "delivery-scope".into(),
            event_dedupe_key: "event-scope".into(),
            event: WaffoWebhookEvent {
                id: "delivery-1".into(),
                timestamp: "2026-08-09T00:00:00Z".into(),
                event_type: "order.completed".into(),
                event_id: "order-1".into(),
                store_id: "STO_0123456789ABCDEFGHIJKL".into(),
                store_name: "Example".into(),
                mode: WaffoWebhookMode::Test,
                data: json!({}),
            },
        };
        let mut deliveries = FakeClaims::default();
        let mut events = FakeClaims::default();
        let mut orders = FakeOrders::default();

        assert_eq!(
            project_verified(&verified, &mut deliveries, &mut events, &mut orders),
            ProjectionOutcome::Handled
        );
        assert_eq!(
            project_verified(&verified, &mut deliveries, &mut events, &mut orders),
            ProjectionOutcome::Duplicate
        );
        assert_eq!(orders.0, ["order-1"]);
    }
}

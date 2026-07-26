use crate::{ContributionRegistration, ContributionTypeRegistration, RegistrationOwner, Shared};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    sync::Arc,
};

/// Mutable, typed multi-binding registry used while plugins are installed.
///
/// Services represent a single authoritative implementation for a type. Contributions
/// represent zero or more independent implementations, such as HTTP modules, notification
/// sinks, health checks, or event observers.
#[derive(Default)]
pub struct ContributionCollection {
    values: HashMap<TypeId, Vec<Arc<dyn Any + Send + Sync>>>,
    registrations: HashMap<TypeId, ContributionTypeRegistration>,
    next_installation_index: usize,
}

impl std::fmt::Debug for ContributionCollection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ContributionCollection")
            .field("contribution_types", &self.values.len())
            .field(
                "contribution_count",
                &self.values.values().map(Vec::len).sum::<usize>(),
            )
            .field("registration_types", &self.registrations.len())
            .field("next_installation_index", &self.next_installation_index)
            .finish()
    }
}

impl ContributionCollection {
    /// Adds one concrete contribution.
    pub fn push<T>(&mut self, value: Arc<T>)
    where
        T: Any + Send + Sync,
    {
        self.push_owned(value, RegistrationOwner::application());
    }

    pub(crate) fn push_owned<T>(&mut self, value: Arc<T>, owner: RegistrationOwner)
    where
        T: Any + Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        self.values.entry(type_id).or_default().push(value);
        self.registrations
            .entry(type_id)
            .or_insert_with(|| ContributionTypeRegistration {
                rust_type: std::any::type_name::<T>(),
                registrations: Vec::new(),
            })
            .registrations
            .push(ContributionRegistration {
                owner,
                installation_index: self.next_installation_index,
            });
        self.next_installation_index += 1;
    }

    /// Adds one trait-object contribution while retaining a typed retrieval API.
    pub fn push_shared<T>(&mut self, value: Arc<T>)
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.push_shared_owned(value, RegistrationOwner::application());
    }

    pub(crate) fn push_shared_owned<T>(&mut self, value: Arc<T>, owner: RegistrationOwner)
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.push_owned(Arc::new(Shared::new(value)), owner);
    }

    /// Returns all concrete contributions currently registered for `T`.
    pub fn get<T>(&self) -> Vec<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.values
            .get(&TypeId::of::<T>())
            .into_iter()
            .flatten()
            .filter_map(|value| Arc::clone(value).downcast::<T>().ok())
            .collect()
    }

    /// Returns all trait-object contributions currently registered for `T`.
    pub fn get_shared<T>(&self) -> Vec<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get::<Shared<T>>()
            .into_iter()
            .map(|value| value.inner())
            .collect()
    }

    pub fn freeze(self) -> FrozenContributions {
        let mut registrations = self.registrations.into_values().collect::<Vec<_>>();
        registrations.sort_by(|left, right| {
            let left_index = left
                .registrations
                .first()
                .map_or(usize::MAX, |registration| registration.installation_index);
            let right_index = right
                .registrations
                .first()
                .map_or(usize::MAX, |registration| registration.installation_index);
            left.rust_type
                .cmp(right.rust_type)
                .then_with(|| left_index.cmp(&right_index))
        });
        FrozenContributions {
            values: self.values,
            registrations,
        }
    }
}

/// Owner-bound contribution registrar exposed by `PluginContext`.
///
/// Registration order is global across contribution types and deterministic for a repeated
/// composition of the same application and plugin graph.
pub struct ContributionRegistrar<'a> {
    pub(crate) contributions: &'a mut ContributionCollection,
    pub(crate) owner: RegistrationOwner,
}

impl fmt::Debug for ContributionRegistrar<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContributionRegistrar")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

impl ContributionRegistrar<'_> {
    pub fn push<T>(&mut self, value: Arc<T>)
    where
        T: Any + Send + Sync,
    {
        self.contributions.push_owned(value, self.owner.clone());
    }

    pub fn push_shared<T>(&mut self, value: Arc<T>)
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.contributions
            .push_shared_owned(value, self.owner.clone());
    }

    pub fn get<T>(&self) -> Vec<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.contributions.get::<T>()
    }

    pub fn get_shared<T>(&self) -> Vec<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.contributions.get_shared::<T>()
    }
}

/// Immutable contribution registry exposed by a composed application.
#[derive(Clone, Default)]
pub struct FrozenContributions {
    values: HashMap<TypeId, Vec<Arc<dyn Any + Send + Sync>>>,
    registrations: Vec<ContributionTypeRegistration>,
}

impl std::fmt::Debug for FrozenContributions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrozenContributions")
            .field("contribution_types", &self.values.len())
            .field(
                "contribution_count",
                &self.values.values().map(Vec::len).sum::<usize>(),
            )
            .field("registration_types", &self.registrations.len())
            .finish()
    }
}

impl FrozenContributions {
    /// Returns metadata grouped by Rust type while preserving registration order.
    pub fn registrations(&self) -> &[ContributionTypeRegistration] {
        &self.registrations
    }

    pub fn get<T>(&self) -> Vec<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        self.values
            .get(&TypeId::of::<T>())
            .into_iter()
            .flatten()
            .filter_map(|value| Arc::clone(value).downcast::<T>().ok())
            .collect()
    }

    pub fn get_shared<T>(&self) -> Vec<Arc<T>>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get::<Shared<T>>()
            .into_iter()
            .map(|value| value.inner())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    trait Named: Send + Sync {
        fn name(&self) -> &'static str;
    }

    #[derive(Debug)]
    struct First;
    impl Named for First {
        fn name(&self) -> &'static str {
            "first"
        }
    }

    #[derive(Debug)]
    struct Second;
    impl Named for Second {
        fn name(&self) -> &'static str {
            "second"
        }
    }

    #[test]
    fn shared_contributions_preserve_registration_order() {
        let mut values = ContributionCollection::default();
        values.push_shared::<dyn Named>(Arc::new(First));
        values.push_shared::<dyn Named>(Arc::new(Second));

        let frozen = values.freeze();
        let names = frozen
            .get_shared::<dyn Named>()
            .into_iter()
            .map(|value| value.name())
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second"]);
        assert_eq!(frozen.registrations().len(), 1);
        let metadata = &frozen.registrations()[0];
        assert_eq!(
            metadata.rust_type,
            std::any::type_name::<Shared<dyn Named>>()
        );
        assert_eq!(
            metadata
                .registrations
                .iter()
                .map(|registration| registration.installation_index)
                .collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(
            metadata
                .registrations
                .iter()
                .all(|registration| registration.owner.is_application())
        );
    }
}

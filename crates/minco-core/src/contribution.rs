use crate::Shared;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
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
            .finish()
    }
}

impl ContributionCollection {
    /// Adds one concrete contribution.
    pub fn push<T>(&mut self, value: Arc<T>)
    where
        T: Any + Send + Sync,
    {
        self.values
            .entry(TypeId::of::<T>())
            .or_default()
            .push(value);
    }

    /// Adds one trait-object contribution while retaining a typed retrieval API.
    pub fn push_shared<T>(&mut self, value: Arc<T>)
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.push(Arc::new(Shared::new(value)));
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
        FrozenContributions {
            values: self.values,
        }
    }
}

/// Immutable contribution registry exposed by a composed application.
#[derive(Clone, Default)]
pub struct FrozenContributions {
    values: HashMap<TypeId, Vec<Arc<dyn Any + Send + Sync>>>,
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
            .finish()
    }
}

impl FrozenContributions {
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

        let names = values
            .freeze()
            .get_shared::<dyn Named>()
            .into_iter()
            .map(|value| value.name())
            .collect::<Vec<_>>();

        assert_eq!(names, ["first", "second"]);
    }
}

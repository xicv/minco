use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};
use thiserror::Error;

/// Sized wrapper that lets the service and contribution registries store an `Arc<dyn Trait>`
/// without introducing a global service locator or stringly typed key.
#[derive(Clone)]
pub struct Shared<T: ?Sized + Send + Sync + 'static> {
    inner: Arc<T>,
}

impl<T: ?Sized + Send + Sync + 'static> Shared<T> {
    pub const fn new(inner: Arc<T>) -> Self {
        Self { inner }
    }

    pub fn inner(&self) -> Arc<T> {
        Arc::clone(&self.inner)
    }
}

impl<T: ?Sized + Send + Sync + 'static> std::fmt::Debug for Shared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Shared")
            .field("service", &std::any::type_name::<T>())
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct ServiceCollection {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ServiceCollection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceCollection")
            .field("service_count", &self.services.len())
            .finish()
    }
}

impl ServiceCollection {
    pub fn insert<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: Any + Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        if self.services.contains_key(&type_id) {
            return Err(ServiceError::Duplicate(std::any::type_name::<T>()));
        }
        self.services.insert(type_id, value);
        Ok(())
    }

    pub fn insert_shared<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert(Arc::new(Shared::new(value)))
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.services.contains_key(&TypeId::of::<T>())
    }

    pub fn contains_shared<T>(&self) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.contains::<Shared<T>>()
    }

    pub fn get<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        get_service(&self.services)
    }

    pub fn get_optional<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        get_optional_service(&self.services)
    }

    pub fn get_shared<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get::<Shared<T>>().map(|service| service.inner())
    }

    pub fn get_optional_shared<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get_optional::<Shared<T>>()
            .map(|service| service.map(|value| value.inner()))
    }

    pub fn freeze(self) -> FrozenServices {
        FrozenServices {
            services: self.services,
        }
    }
}

#[derive(Clone, Default)]
pub struct FrozenServices {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for FrozenServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrozenServices")
            .field("service_count", &self.services.len())
            .finish()
    }
}

impl FrozenServices {
    pub fn get<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        get_service(&self.services)
    }

    pub fn get_optional<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        get_optional_service(&self.services)
    }

    pub fn get_shared<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get::<Shared<T>>().map(|service| service.inner())
    }

    pub fn get_optional_shared<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.get_optional::<Shared<T>>()
            .map(|service| service.map(|value| value.inner()))
    }
}

fn get_service<T>(
    services: &HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
) -> Result<Arc<T>, ServiceError>
where
    T: Any + Send + Sync,
{
    services
        .get(&TypeId::of::<T>())
        .cloned()
        .ok_or_else(|| ServiceError::Missing(std::any::type_name::<T>()))?
        .downcast::<T>()
        .map_err(|_| ServiceError::TypeMismatch(std::any::type_name::<T>()))
}

fn get_optional_service<T>(
    services: &HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
) -> Result<Option<Arc<T>>, ServiceError>
where
    T: Any + Send + Sync,
{
    services
        .get(&TypeId::of::<T>())
        .cloned()
        .map(|value| {
            value
                .downcast::<T>()
                .map_err(|_| ServiceError::TypeMismatch(std::any::type_name::<T>()))
        })
        .transpose()
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ServiceError {
    #[error("service is already registered: {0}")]
    Duplicate(&'static str),
    #[error("service is not registered: {0}")]
    Missing(&'static str),
    #[error("service has an unexpected concrete type: {0}")]
    TypeMismatch(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;

    trait Greeting: Send + Sync {
        fn text(&self) -> &'static str;
    }

    #[derive(Debug)]
    struct Hello;
    impl Greeting for Hello {
        fn text(&self) -> &'static str {
            "hello"
        }
    }

    #[test]
    fn typed_services_are_frozen_and_retrieved_without_string_keys() {
        let mut services = ServiceCollection::default();
        services.insert(Arc::new(String::from("hello"))).unwrap();
        let frozen = services.freeze();
        assert_eq!(&*frozen.get::<String>().unwrap(), "hello");
    }

    #[test]
    fn trait_object_services_are_registered_without_double_arc_call_sites() {
        let mut services = ServiceCollection::default();
        services
            .insert_shared::<dyn Greeting>(Arc::new(Hello))
            .unwrap();
        assert_eq!(
            services.get_shared::<dyn Greeting>().unwrap().text(),
            "hello"
        );
    }

    #[test]
    fn optional_service_lookup_distinguishes_absence_from_type_errors() {
        let services = ServiceCollection::default();
        assert!(services.get_optional::<String>().unwrap().is_none());
        assert!(
            services
                .get_optional_shared::<dyn Greeting>()
                .unwrap()
                .is_none()
        );

        let frozen = services.freeze();
        assert!(frozen.get_optional::<String>().unwrap().is_none());
        assert!(
            frozen
                .get_optional_shared::<dyn Greeting>()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn duplicate_service_types_are_rejected() {
        let mut services = ServiceCollection::default();
        services.insert(Arc::new(1_u64)).unwrap();
        assert!(matches!(
            services.insert(Arc::new(2_u64)),
            Err(ServiceError::Duplicate(_))
        ));
    }
}

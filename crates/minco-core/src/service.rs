use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt,
    sync::Arc,
};
use thiserror::Error;

use crate::{RegistrationOwner, ServiceRegistration};

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
    registrations: HashMap<TypeId, (ServiceRegistration, usize)>,
    next_installation_index: usize,
}

impl std::fmt::Debug for ServiceCollection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServiceCollection")
            .field("service_count", &self.services.len())
            .field("registration_count", &self.registrations.len())
            .field("next_installation_index", &self.next_installation_index)
            .finish()
    }
}

impl ServiceCollection {
    pub fn insert<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: Any + Send + Sync,
    {
        self.insert_owned(value, RegistrationOwner::application())
    }

    pub(crate) fn insert_owned<T>(
        &mut self,
        value: Arc<T>,
        attempted_owner: RegistrationOwner,
    ) -> Result<(), ServiceError>
    where
        T: Any + Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        let rust_type = std::any::type_name::<T>();
        if let Some((first, _)) = self.registrations.get(&type_id) {
            return Err(ServiceError::Duplicate(DuplicateServiceRegistration {
                rust_type,
                first_owner: first.owner.clone(),
                attempted_owner,
            }));
        }
        self.services.insert(type_id, value);
        self.registrations.insert(
            type_id,
            (
                ServiceRegistration {
                    rust_type,
                    owner: attempted_owner,
                },
                self.next_installation_index,
            ),
        );
        self.next_installation_index += 1;
        Ok(())
    }

    pub fn insert_shared<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_shared_owned(value, RegistrationOwner::application())
    }

    pub(crate) fn insert_shared_owned<T>(
        &mut self,
        value: Arc<T>,
        owner: RegistrationOwner,
    ) -> Result<(), ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.insert_owned(Arc::new(Shared::new(value)), owner)
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
        let mut registrations = self.registrations.into_values().collect::<Vec<_>>();
        registrations.sort_by(|(left, left_index), (right, right_index)| {
            left.rust_type
                .cmp(right.rust_type)
                .then_with(|| left_index.cmp(right_index))
        });
        let registrations = registrations
            .into_iter()
            .map(|(registration, _)| registration)
            .collect();
        FrozenServices {
            services: self.services,
            registrations,
        }
    }
}

/// Owner-bound singleton registrar exposed by `PluginContext`.
///
/// The owner is created by `PluginManager`; plugins can register and retrieve typed values but
/// cannot select or spoof a registration owner.
pub struct ServiceRegistrar<'a> {
    pub(crate) services: &'a mut ServiceCollection,
    pub(crate) owner: RegistrationOwner,
}

impl fmt::Debug for ServiceRegistrar<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServiceRegistrar")
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

impl ServiceRegistrar<'_> {
    pub fn insert<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: Any + Send + Sync,
    {
        self.services.insert_owned(value, self.owner.clone())
    }

    pub fn insert_shared<T>(&mut self, value: Arc<T>) -> Result<(), ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.insert_shared_owned(value, self.owner.clone())
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.services.contains::<T>()
    }

    pub fn contains_shared<T>(&self) -> bool
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.contains_shared::<T>()
    }

    pub fn get<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        self.services.get::<T>()
    }

    pub fn get_optional<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: Any + Send + Sync,
    {
        self.services.get_optional::<T>()
    }

    pub fn get_shared<T>(&self) -> Result<Arc<T>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.get_shared::<T>()
    }

    pub fn get_optional_shared<T>(&self) -> Result<Option<Arc<T>>, ServiceError>
    where
        T: ?Sized + Send + Sync + 'static,
    {
        self.services.get_optional_shared::<T>()
    }
}

#[derive(Clone, Default)]
pub struct FrozenServices {
    services: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    registrations: Vec<ServiceRegistration>,
}

impl std::fmt::Debug for FrozenServices {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrozenServices")
            .field("service_count", &self.services.len())
            .field("registration_count", &self.registrations.len())
            .finish()
    }
}

impl FrozenServices {
    /// Returns deterministic metadata without exposing service values.
    pub fn registrations(&self) -> &[ServiceRegistration] {
        &self.registrations
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateServiceRegistration {
    pub rust_type: &'static str,
    pub first_owner: RegistrationOwner,
    pub attempted_owner: RegistrationOwner,
}

impl fmt::Display for DuplicateServiceRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} (first owner: {}, attempted owner: {})",
            self.rust_type, self.first_owner, self.attempted_owner
        )
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ServiceError {
    #[error("service is already registered: {0}")]
    Duplicate(DuplicateServiceRegistration),
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
        let Err(ServiceError::Duplicate(duplicate)) = services.insert(Arc::new(2_u64)) else {
            panic!("duplicate must fail");
        };
        assert_eq!(duplicate.rust_type, std::any::type_name::<u64>());
        assert!(duplicate.first_owner.is_application());
        assert!(duplicate.attempted_owner.is_application());
        assert_eq!(
            duplicate.to_string(),
            "u64 (first owner: application, attempted owner: application)"
        );
    }
}

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Arc,
};
use thiserror::Error;

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

    pub fn contains<T>(&self) -> bool
    where
        T: Any + Send + Sync,
    {
        self.services.contains_key(&TypeId::of::<T>())
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
        self.services
            .get(&TypeId::of::<T>())
            .cloned()
            .ok_or(ServiceError::Missing(std::any::type_name::<T>()))?
            .downcast::<T>()
            .map_err(|_| ServiceError::TypeMismatch(std::any::type_name::<T>()))
    }
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

    #[test]
    fn typed_services_are_frozen_and_retrieved_without_string_keys() {
        let mut services = ServiceCollection::default();
        services
            .insert(Arc::new(String::from("hello")))
            .unwrap();
        let frozen = services.freeze();
        assert_eq!(&*frozen.get::<String>().unwrap(), "hello");
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

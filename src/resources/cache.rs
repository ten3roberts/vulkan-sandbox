use std::{any, collections::HashMap};

use generational_arena::Arena;

use super::Error;
use super::Handle;

pub struct ResourceCache<R> {
    resources: Arena<R>,
    name_cache: HashMap<String, Handle<R>>,
}

impl<R> ResourceCache<R> {
    pub fn new() -> Self {
        Self {
            resources: Arena::new(),
            name_cache: HashMap::new(),
        }
    }

    /// Get a resource from cache by name. Returns a `Error::NotFound` if not found.
    pub fn get<S>(&self, name: S) -> Result<Handle<R>, Error>
    where
        S: AsRef<str> + Into<String>,
    {
        match self.name_cache.get(name.as_ref()) {
            Some(handle) => Ok(*handle),
            None => Err(Error::NotFound(any::type_name::<R>(), name.into())),
        }
    }

    /// Get a resource from the cache or insert resource computed from fallible closure. Returns
    /// Err if closure returns Err.
    pub fn insert<S, E, F: FnOnce() -> Result<R, E>>(
        &mut self,
        name: S,
        op: F,
    ) -> Result<Handle<R>, E>
    where
        S: AsRef<str> + Into<String>,
    {
        if let Some(resource) = self.name_cache.get(name.as_ref()) {
            return Ok(*resource);
        }

        let resource = op()?;
        let handle = self.resources.insert(resource).into();

        self.name_cache.insert(name.into(), handle);
        Ok(handle)
    }

    /// Returns a reference to the underlying resource pointed to by handle. Returns
    /// `Error::InvalidInvalidHandle` if handle is no longer valid.
    pub fn raw(&self, handle: Handle<R>) -> Result<&R, Error> {
        match self.resources.get(handle.into()) {
            Some(resource) => Ok(resource),
            None => Err(Error::InvalidHandle(std::any::type_name::<R>())),
        }
    }
}

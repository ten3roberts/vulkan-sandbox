use std::collections::HashMap;

use generational_arena::Arena;

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

    /// Get a resource from cache.
    pub fn get(&self, name: &str) -> Option<Handle<R>> {
        self.name_cache.get(name).map(|handle| *handle)
    }

    /// Get a resource from the cache or insert resource computed from fallible closure. Returns
    /// Err if closure returns Err.
    pub fn insert<E, F: FnOnce() -> Result<R, E>>(
        &mut self,
        name: &str,
        op: F,
    ) -> Result<Handle<R>, E> {
        if let Some(resource) = self.name_cache.get(name) {
            return Ok(*resource);
        }
        let resource = op()?;
        let handle = self.resources.insert(resource).into();

        self.name_cache.insert(name.to_owned(), handle);
        Ok(handle)
    }

    /// Returns a reference to the underlying resource pointed to by handle. Returns None if handle
    /// is no longer valid.
    pub fn raw(&self, handle: Handle<R>) -> Option<&R> {
        self.resources.get(handle.into())
    }
}

//! Wraps an arena index to provide a typesafe handle.

use generational_arena::Index;
use std::hash::Hash;
use std::marker::PhantomData;

pub struct Handle<R>(Index, PhantomData<R>);

impl<R> Clone for Handle<R> {
    fn clone(&self) -> Self {
        Self(self.0, PhantomData)
    }
}

impl<R> Copy for Handle<R> {}

impl<R> PartialEq for Handle<R> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<R> Eq for Handle<R> {}

impl<R> Hash for Handle<R> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<R> From<Index> for Handle<R> {
    fn from(index: Index) -> Self {
        Self(index, PhantomData)
    }
}

impl<R> Into<Index> for Handle<R> {
    fn into(self) -> Index {
        self.0
    }
}

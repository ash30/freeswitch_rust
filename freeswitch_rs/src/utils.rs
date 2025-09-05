use std::marker::PhantomData;

// Handles are wrappers over ptrs where we cannot
// guarantee any lifetime!
pub struct FSHandle<T> {
    pub(crate) ptr: *mut T,
}
unsafe impl<T> Send for FSHandle<T> where T: Send {}

// Scoped handles lifetime is usually defined by the
// 'owning' object ( usually the session )
pub struct FSScopedHandle<'a, T> {
    pub(crate) ptr: *mut T,
    lifetime: PhantomData<&'a T>,
}

impl<'a, T> FSScopedHandle<'a, T> {
    pub fn from_raw(ptr: *mut T) -> FSScopedHandle<'a, T> {
        Self {
            ptr,
            lifetime: PhantomData {},
        }
    }
}

pub struct FSScopedHandleMut<'a, T> {
    pub(crate) ptr: *mut T,
    lifetime: PhantomData<&'a mut T>,
}

impl<'a, T> FSScopedHandleMut<'a, T> {
    pub fn from_raw(ptr: *mut T) -> FSScopedHandleMut<'a, T> {
        Self {
            ptr,
            lifetime: PhantomData {},
        }
    }
}

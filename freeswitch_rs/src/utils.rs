use std::marker::PhantomData;

pub struct FSHandle<T>{ 
    pub(crate) ptr: *mut T,
}
unsafe impl<T> Send for FSHandle<T> where T:Send {}

pub struct FSObject<'a, T> {
    pub(crate) ptr: *mut T,
    lifetime: PhantomData<&'a T>
}

impl <'a, T> FSObject<'a,T> {
    pub fn from_raw(ptr:* mut T) -> FSObject<'a,T> {
        Self {
            ptr,
            lifetime: PhantomData {}
        }
    }
}

pub struct FSObjectMut<'a, T> {
    pub(crate) ptr: *mut T,
    lifetime: PhantomData<&'a mut T>
}

impl <'a, T> FSObjectMut<'a,T> {
    pub fn from_raw(ptr:* mut T) -> FSObjectMut<'a,T> {
        Self {
            ptr,
            lifetime: PhantomData {}
        }
    }
}


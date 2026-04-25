use std::{any::Any, collections::HashMap};

use crate::folds::{Fold, FoldKey};

pub trait EnforceFn<T>: 'static {
    fn check(
        self,
        states: &HashMap<FoldKey, Box<dyn Any>>,
        keys: Box<dyn Any>,
    ) -> anyhow::Result<()>;
}

macro_rules! impl_enforce_fn {
    ( $( $T:ident : $n:tt ),* => $keys_ty:ty ) => {
        impl<T, $($T),*> EnforceFn<($($T,)*)> for T
        where
            T: FnOnce($($T::State),*) -> anyhow::Result<()> + 'static,
            $(
                $T: Fold,
                $T::State: Clone,
            )*
        {
            #[allow(unused)]
            fn check(
                self,
                states: &HashMap<FoldKey, Box<dyn Any>>,
                keys: Box<dyn Any>,
            ) -> anyhow::Result<()> {
                let keys: $keys_ty = *keys.downcast().unwrap();
                self(
                    $(
                        states
                            .get(&keys.$n)
                            .unwrap()
                            .downcast_ref::<$T::State>()
                            .unwrap()
                            .clone(),
                    )*
                )
            }
        }
    };
}

impl_enforce_fn!(=> ());
impl_enforce_fn!(A:0 => (FoldKey,));
impl_enforce_fn!(A:0, B:1 => (FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2 => (FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3 => (FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));

pub trait EnforceWithInputFn<I, T>: 'static {
    fn check(
        self,
        input: I,
        states: &HashMap<FoldKey, Box<dyn Any>>,
        keys: Box<dyn Any>,
    ) -> anyhow::Result<()>;
}

macro_rules! impl_enforce_with_input_fn {
    ( $( $T:ident : $n:tt ),* => $keys_ty:ty ) => {
        impl<In, T, $($T),*> EnforceWithInputFn<In, ($($T,)*)> for T
        where
            T: FnOnce(In, $($T::State),*) -> anyhow::Result<()> + 'static,
            $(
                $T: Fold,
                $T::State: Clone,
            )*
        {
            #[allow(unused)]
            fn check(
                self,
                input: In,
                states: &HashMap<FoldKey, Box<dyn Any>>,
                keys: Box<dyn Any>,
            ) -> anyhow::Result<()> {
                let keys: $keys_ty = *keys.downcast().unwrap();
                self(
                    input,
                    $(
                        states
                            .get(&keys.$n)
                            .unwrap()
                            .downcast_ref::<$T::State>()
                            .unwrap()
                            .clone(),
                    )*
                )
            }
        }
    };
}

impl_enforce_with_input_fn!(=> ());
impl_enforce_with_input_fn!(A:0 => (FoldKey,));
impl_enforce_with_input_fn!(A:0, B:1 => (FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2 => (FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3 => (FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));

pub trait EnforceRefFn<T>: 'static {
    fn check(
        self,
        states: &HashMap<FoldKey, Box<dyn Any>>,
        keys: Box<dyn Any>,
    ) -> anyhow::Result<()>;
}

macro_rules! impl_enforce_ref_fn {
    ( $( $T:ident : $n:tt ),* => $keys_ty:ty ) => {
        impl<T, $($T),*> EnforceRefFn<($($T,)*)> for T
        where
            T: for<'s> FnOnce($(&'s $T::State),*) -> anyhow::Result<()> + 'static,
            $(
                $T: Fold,
            )*
        {
            #[allow(unused)]
            fn check(
                self,
                states: &HashMap<FoldKey, Box<dyn Any>>,
                keys: Box<dyn Any>,
            ) -> anyhow::Result<()> {
                let keys: $keys_ty = *keys.downcast().unwrap();
                self(
                    $(
                        states
                            .get(&keys.$n)
                            .unwrap()
                            .downcast_ref::<$T::State>()
                            .unwrap(),
                    )*
                )
            }
        }
    };
}

impl_enforce_ref_fn!(=> ());
impl_enforce_ref_fn!(A:0 => (FoldKey,));
impl_enforce_ref_fn!(A:0, B:1 => (FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2 => (FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3 => (FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));

pub trait EnforceWithInputRefFn<I, T>: 'static {
    fn check(
        self,
        input: &I,
        states: &HashMap<FoldKey, Box<dyn Any>>,
        keys: Box<dyn Any>,
    ) -> anyhow::Result<()>;
}

macro_rules! impl_enforce_with_input_ref_fn {
    ( $( $T:ident : $n:tt ),* => $keys_ty:ty ) => {
        impl<In, T, $($T),*> EnforceWithInputRefFn<In, ($($T,)*)> for T
        where
            T: for<'i, 's> FnOnce(&'i In, $(&'s $T::State),*) -> anyhow::Result<()> + 'static,
            $(
                $T: Fold,
            )*
        {
            #[allow(unused)]
            fn check(
                self,
                input: &In,
                states: &HashMap<FoldKey, Box<dyn Any>>,
                keys: Box<dyn Any>,
            ) -> anyhow::Result<()> {
                let keys: $keys_ty = *keys.downcast().unwrap();
                self(
                    input,
                    $(
                        states
                            .get(&keys.$n)
                            .unwrap()
                            .downcast_ref::<$T::State>()
                            .unwrap(),
                    )*
                )
            }
        }
    };
}

impl_enforce_with_input_ref_fn!(=> ());
impl_enforce_with_input_ref_fn!(A:0 => (FoldKey,));
impl_enforce_with_input_ref_fn!(A:0, B:1 => (FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2 => (FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3 => (FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));
impl_enforce_with_input_ref_fn!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11 => (FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey, FoldKey));

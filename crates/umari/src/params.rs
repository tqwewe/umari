use uuid::Uuid;

use crate::sqlite::SqliteValue;

pub trait Params {
    fn into_params(self) -> Vec<SqliteValue>;
}

impl<T: Into<SqliteValue>> Params for Vec<T> {
    fn into_params(self) -> Vec<SqliteValue> {
        self.into_iter().map(|value| value.into()).collect()
    }
}

impl Params for () {
    fn into_params(self) -> Vec<SqliteValue> {
        vec![]
    }
}

impl<A> Params for (A,)
where
    A: Into<SqliteValue>,
{
    fn into_params(self) -> Vec<SqliteValue> {
        vec![self.0.into()]
    }
}

macro_rules! single_tuple_impl {
    ($(($field:tt $ftype:ident)),* $(,)?) => {
        impl<$($ftype,)*> Params for ($($ftype,)*) where $($ftype: Into<SqliteValue>,)* {
            fn into_params(self) -> Vec<SqliteValue> {
                vec![
                    $(
                        <$ftype as Into<SqliteValue>>::into(self.$field)
                    ),+
                ]
            }
        }
    }
}

single_tuple_impl!((0 A), (1 B));
single_tuple_impl!((0 A), (1 B), (2 C));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N), (14 O));
single_tuple_impl!((0 A), (1 B), (2 C), (3 D), (4 E), (5 F), (6 G), (7 H), (8 I), (9 J), (10 K), (11 L), (12 M), (13 N), (14 O), (15 P));

macro_rules! impl_for_array_ref {
    ($($N:literal)+) => {$(
        impl<T> Params for &[T; $N]
        where
            for<'a> &'a T: Into<SqliteValue>
        {
            fn into_params(self) -> Vec<SqliteValue> {
                self.into_iter().map(|value| value.into()).collect()
            }
        }

        impl<T> Params for [T; $N]
        where
            T: Into<SqliteValue>
        {
            fn into_params(self) -> Vec<SqliteValue> {
                self.into_iter().map(|value| value.into()).collect()
            }
        }
    )+};
}

impl_for_array_ref!(
    1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17
    18 19 20 21 22 23 24 25 26 27 28 29 30 31 32
);

impl From<bool> for SqliteValue {
    #[inline]
    fn from(i: bool) -> Self {
        Self::Integer(i as i64)
    }
}

impl From<isize> for SqliteValue {
    #[inline]
    fn from(i: isize) -> Self {
        Self::Integer(i as i64)
    }
}

macro_rules! from_i64(
    ($t:ty) => (
        impl From<$t> for SqliteValue {
            #[inline]
            fn from(i: $t) -> SqliteValue {
                SqliteValue::Integer(i64::from(i))
            }
        }
    )
);

from_i64!(i8);
from_i64!(i16);
from_i64!(i32);
from_i64!(u8);
from_i64!(u16);
from_i64!(u32);

impl From<i64> for SqliteValue {
    #[inline]
    fn from(i: i64) -> Self {
        Self::Integer(i)
    }
}

impl From<f32> for SqliteValue {
    #[inline]
    fn from(f: f32) -> Self {
        Self::Real(f.into())
    }
}

impl From<f64> for SqliteValue {
    #[inline]
    fn from(f: f64) -> Self {
        Self::Real(f)
    }
}

impl From<String> for SqliteValue {
    #[inline]
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<Vec<u8>> for SqliteValue {
    #[inline]
    fn from(v: Vec<u8>) -> Self {
        Self::Blob(v)
    }
}

impl From<Uuid> for SqliteValue {
    fn from(id: Uuid) -> Self {
        Self::Text(id.to_string())
    }
}

impl<T> From<Option<T>> for SqliteValue
where
    T: Into<Self>,
{
    #[inline]
    fn from(v: Option<T>) -> Self {
        match v {
            Some(x) => x.into(),
            None => Self::Null,
        }
    }
}

#[macro_export]
macro_rules! params {
    () => {
        vec![]
    };
    ($($param:expr),+ $(,)?) => {
        vec![$( Into::<SqliteValue>::into($param) ),+]
    };
}

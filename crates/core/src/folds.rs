use serde_json::Value;

use crate::{
    command::EventMeta,
    domain_id::DomainIdBindings,
    error::SerializationError,
    event::{EventDomainId, EventSet},
};

pub trait Fold: Default {
    type Events: EventSet;

    fn apply(&mut self, event: &<Self::Events as EventSet>::Item, meta: EventMeta);
}

macro_rules! impl_tuple_folds {
    ($( $t:ident:$n:tt ),*) => {
        impl<$($t,)*> Fold for ($($t,)*)
        where
            $(
                $t: Fold,
            )*
        {
            type Events = ($($t::Events,)*);

            fn apply(&mut self, event: &<Self::Events as EventSet>::Item, meta: EventMeta) {
                $(
                    if let Some(ref e) = event.$n {
                        self.$n.apply(e, meta);
                    }
                )*
            }
        }
    };
}

impl_tuple_folds!(A:0);
impl_tuple_folds!(A:0, B:1);
impl_tuple_folds!(A:0, B:1, C:2);
impl_tuple_folds!(A:0, B:1, C:2, D:3);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);
impl_tuple_folds!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10, L:11);

pub trait FoldSet: Default {
    fn event_types() -> Vec<&'static str>;
    fn event_domain_ids() -> Vec<EventDomainId>;

    fn apply(
        &mut self,
        event_type: &str,
        data: Value,
        tags: &[String],
        bindings: &DomainIdBindings,
        meta: EventMeta,
    ) -> Result<(), SerializationError>;
}

impl FoldSet for () {
    fn event_types() -> Vec<&'static str> {
        vec![]
    }

    fn event_domain_ids() -> Vec<EventDomainId> {
        vec![]
    }

    fn apply(
        &mut self,
        _event_type: &str,
        _data: Value,
        _tags: &[String],
        _bindings: &DomainIdBindings,
        _meta: EventMeta,
    ) -> Result<(), SerializationError> {
        Ok(())
    }
}

macro_rules! impl_tuple_fold_sets {
    ($( $t:ident:$n:tt ),+) => {
        impl<$($t,)+> FoldSet for ($($t,)+)
        where
            $(
                $t: Fold,
            )+
        {
            fn event_types() -> Vec<&'static str> {
                let mut types = Vec::new();
                $(
                    types.extend_from_slice(&$t::Events::event_types());
                )+
                types
            }

            fn event_domain_ids() -> Vec<EventDomainId> {
                let mut ids = Vec::new();
                $(
                    ids.extend($t::Events::event_domain_ids());
                )+
                ids
            }

            fn apply(
                &mut self,
                event_type: &str,
                data: Value,
                tags: &[String],
                bindings: &DomainIdBindings,
                meta: EventMeta,
            ) -> Result<(), SerializationError> {
                $(
                    if matches_fold_query::<$t>(event_type, tags, bindings)
                        && let Some(event) = $t::Events::from_event(event_type, data.clone()).transpose()?
                    {
                        self.$n.apply(&event, meta);
                    }
                )+
                Ok(())
            }
        }
    };
}

impl_tuple_fold_sets!(A:0);
impl_tuple_fold_sets!(A:0, B:1);
impl_tuple_fold_sets!(A:0, B:1, C:2);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9);
impl_tuple_fold_sets!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7, I:8, J:9, K:10);

pub(crate) fn matches_fold_query<I: Fold>(
    event_type: &str,
    tags: &[String],
    bindings: &DomainIdBindings,
) -> bool {
    let domain_ids = I::Events::event_domain_ids();
    let required = domain_ids.iter().find(|e| e.event_type == event_type);
    let Some(required) = required else {
        return true;
    };

    required.dynamic_fields.iter().all(|field| {
        bindings
            .get(field)
            .map(|values| {
                // same as tags.contains(&format!("{field}:{v}"))), but avoids allocation
                values.iter().any(|v| {
                    tags.iter().any(|tag| {
                        let Some(rest) = tag.strip_prefix(field) else {
                            return false;
                        };

                        let Some(rest) = rest.strip_prefix(':') else {
                            return false;
                        };

                        rest == v
                    })
                })
            })
            .unwrap_or(true)
    }) && required.static_fields.iter().all(|(field, value)| {
        tags.iter().any(|tag| {
            tag.strip_prefix(field)
                .and_then(|r| r.strip_prefix(':'))
                .map(|r| r == *value)
                .unwrap_or(false)
        })
    })
}

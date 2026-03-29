use kameo::prelude::*;

use crate::{module::ModuleError, wit::effect::EffectWorld};

pub struct EffectActor {
    instance: EffectWorld,
}

pub struct EffectActorArgs {}

impl Actor for EffectActor {
    type Args = EffectActorArgs;
    type Error = ModuleError;

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        todo!()
    }
}

use crate::{Artifact, BusEvent, CapabilityDescriptor, ContextPack, Decision, Session};
use schemars::{schema::RootSchema, schema_for};
use std::collections::BTreeMap;

pub fn schema_bundle() -> BTreeMap<&'static str, RootSchema> {
    BTreeMap::from([
        ("Session", schema_for!(Session)),
        ("Artifact", schema_for!(Artifact)),
        ("Decision", schema_for!(Decision)),
        ("BusEvent", schema_for!(BusEvent)),
        ("CapabilityDescriptor", schema_for!(CapabilityDescriptor)),
        ("ContextPack", schema_for!(ContextPack)),
    ])
}

use crate::error::Result;
use kube::{
    Resource, ResourceExt,
    api::{Api, DeleteParams, ListParams, Patch, PatchParams},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, fmt::Debug, hash::Hash};

pub const FIELD_MANAGER: &str = "ech-operator";

pub async fn apply<T>(api: &Api<T>, name: &str, resource: &T) -> Result<()>
where
    T: Resource + Clone + Debug + Serialize + DeserializeOwned,
    <T as Resource>::DynamicType: Default + Eq + Hash + Clone,
{
    let params = PatchParams::apply(FIELD_MANAGER).force();
    api.patch(name, &params, &Patch::Apply(resource)).await?;
    Ok(())
}

pub async fn prune<T>(api: &Api<T>, label_selector: &str, desired: &BTreeSet<String>) -> Result<()>
where
    T: Resource + Clone + DeserializeOwned + Debug,
    <T as Resource>::DynamicType: Default + Eq + Hash + Clone,
{
    let lp = ListParams::default().labels(label_selector);
    for obj in api.list(&lp).await?.items {
        let name = obj.name_any();
        if !desired.contains(name.as_str()) {
            tracing::info!(name = %name, kind = std::any::type_name::<T>(), "deleting stale resource");
            match api.delete(&name, &DeleteParams::foreground()).await {
                Ok(_) => {}
                Err(kube::Error::Api(ref status)) if status.code == 404 => {}
                Err(err) => {
                    tracing::warn!(name = %name, error = %err, "stale resource delete failed")
                }
            }
        }
    }
    Ok(())
}

use std::collections::HashMap;

use crate::{Collection, Environment};

pub(super) fn resolve(
    collection: &Collection,
    environment: Option<&Environment>,
) -> HashMap<String, String> {
    collection
        .variables
        .iter()
        .chain(
            environment
                .into_iter()
                .flat_map(|environment| environment.variables.iter()),
        )
        .filter(|value| value.enabled)
        .map(|value| (value.key.clone(), value.value.clone()))
        .collect()
}

pub(super) fn substitute(input: &str, variables: &HashMap<String, String>) -> String {
    variables
        .iter()
        .fold(input.to_string(), |value, (key, replacement)| {
            value.replace(&format!("{{{{{key}}}}}"), replacement)
        })
}

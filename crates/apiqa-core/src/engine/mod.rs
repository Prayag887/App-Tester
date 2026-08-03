mod evaluation;
mod request;
mod run;
mod transport;
mod variables;

use std::sync::Arc;

use crate::{CleanupResult, Collection, CoreResult, Environment, RetentionPolicy, Run, Store};

pub struct ApiQaEngine {
    store: Arc<Store>,
}

impl ApiQaEngine {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(store),
        }
    }

    pub fn save_collection(&self, value: &Collection) -> CoreResult<()> {
        self.store.save_collection(value)
    }

    pub fn save_project(
        &self,
        collection: &Collection,
        environments: &[Environment],
    ) -> CoreResult<()> {
        self.store.save_project(collection, environments)
    }

    pub fn save_workspace(
        &self,
        collections: &[Collection],
        environments: &[Environment],
    ) -> CoreResult<()> {
        self.store.save_workspace(collections, environments)
    }

    pub fn collections(&self) -> CoreResult<Vec<Collection>> {
        self.store.collections()
    }

    pub fn collection(&self, id: &str) -> CoreResult<Option<Collection>> {
        self.store.collection(id)
    }

    pub fn save_environment(&self, value: &Environment) -> CoreResult<()> {
        self.store.save_environment(value)
    }

    pub fn environments(&self) -> CoreResult<Vec<Environment>> {
        self.store.environments()
    }

    pub fn environment(&self, id: &str) -> CoreResult<Option<Environment>> {
        self.store.environment(id)
    }

    pub fn runs(&self, collection_id: Option<&str>) -> CoreResult<Vec<Run>> {
        self.store.runs(collection_id)
    }

    pub fn run(&self, id: &str) -> CoreResult<Option<Run>> {
        self.store.run(id)
    }

    pub fn run_summaries(&self, collection_id: Option<&str>) -> CoreResult<Vec<Run>> {
        self.store.run_summaries(collection_id)
    }

    pub fn run_count(&self) -> CoreResult<u64> {
        self.store.run_count()
    }

    pub fn set_run_pinned(&self, id: &str, pinned: bool) -> CoreResult<()> {
        self.store.set_run_pinned(id, pinned)
    }

    pub fn retention_policy(&self) -> CoreResult<RetentionPolicy> {
        self.store.retention_policy()
    }

    pub fn set_retention_policy(&self, policy: &RetentionPolicy) -> CoreResult<()> {
        self.store.set_retention_policy(policy)
    }

    pub fn cleanup_history(&self, policy: &RetentionPolicy) -> CoreResult<CleanupResult> {
        self.store.cleanup_history(policy)
    }
}

use super::PostGateApi;
use crate::error::Result;

impl PostGateApi {
    pub async fn list_values(&self) -> Result<Vec<crate::values::ValueEntry>> {
        self.state.ensure_values_loaded().await?;
        let db = self.state.get_database().await?;
        db.list_values().await
    }

    pub async fn save_value(&self, name: &str, content: &str) -> Result<crate::values::ValueEntry> {
        self.state.ensure_values_loaded().await?;
        let db = self.state.get_database().await?;
        let entry = db.upsert_value(name, content).await?;
        self.state
            .values_store
            .insert(entry.name.clone(), entry.content.clone());
        Ok(entry)
    }

    pub async fn rename_value(
        &self,
        old_name: &str,
        new_name: &str,
    ) -> Result<crate::values::ValueEntry> {
        self.state.ensure_values_loaded().await?;
        let db = self.state.get_database().await?;
        let entry = db.rename_value(old_name, new_name).await?;
        self.state.values_store.remove(old_name);
        self.state
            .values_store
            .insert(entry.name.clone(), entry.content.clone());
        Ok(entry)
    }

    pub async fn delete_value(&self, name: &str) -> Result<bool> {
        self.state.ensure_values_loaded().await?;
        let db = self.state.get_database().await?;
        let removed = db.delete_value(name).await?;
        self.state.values_store.remove(name);
        Ok(removed)
    }
}

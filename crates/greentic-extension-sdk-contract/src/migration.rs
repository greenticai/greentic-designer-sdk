//! v1 → v2 migration helpers. Consumer extensions invoke `migrate_v0_4_x_value`
//! during build.sh to translate their describe.json forward; gtdx-cli will
//! call the same helper in `gtdx migrate` (Phase E).

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub warnings: Vec<String>,
    pub dropped_keys: Vec<String>,
}

impl MigrationReport {
    pub fn warn<S: Into<String>>(&mut self, msg: S) {
        self.warnings.push(msg.into());
    }

    pub fn dropped<S: Into<String>>(&mut self, key: S) {
        self.dropped_keys.push(key.into());
    }
}

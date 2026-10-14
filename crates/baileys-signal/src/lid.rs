use std::collections::HashMap;
use std::sync::RwLock;

/// Manages resolution and bidirectional mapping between Phone Number JIDs (`user@s.whatsapp.net`)
/// and Linked Identity JIDs (`user@lid`).
#[derive(Debug, Default)]
pub struct LidMappingStore {
    pn_to_lid: RwLock<HashMap<String, String>>,
    lid_to_pn: RwLock<HashMap<String, String>>,
}

impl LidMappingStore {
    pub fn new() -> Self {
        Self {
            pn_to_lid: RwLock::new(HashMap::new()),
            lid_to_pn: RwLock::new(HashMap::new()),
        }
    }

    pub fn store_mapping(&self, pn_jid: &str, lid_jid: &str) {
        let mut pn_map = self.pn_to_lid.write().unwrap();
        let mut lid_map = self.lid_to_pn.write().unwrap();

        pn_map.insert(pn_jid.to_string(), lid_jid.to_string());
        lid_map.insert(lid_jid.to_string(), pn_jid.to_string());
    }

    pub fn get_lid_for_pn(&self, pn_jid: &str) -> Option<String> {
        let pn_map = self.pn_to_lid.read().unwrap();
        pn_map.get(pn_jid).cloned()
    }

    pub fn get_pn_for_lid(&self, lid_jid: &str) -> Option<String> {
        let lid_map = self.lid_to_pn.read().unwrap();
        lid_map.get(lid_jid).cloned()
    }
}

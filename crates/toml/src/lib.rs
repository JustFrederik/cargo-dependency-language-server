use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use update::{now, read_data, update_thread};

mod git;
mod update;

pub struct TomlData {
    last_checked: Duration,
    updating: bool,
    pub data: HashMap<String, (String, Vec<String>)>,
}

impl TomlData {
    pub fn new(path: &Path) -> Arc<Mutex<Self>> {
        git::git(path);
        let data = read_data(path);
        let sel = Arc::new(Mutex::new(Self {
            last_checked: now(),
            updating: false,
            data,
        }));
        update_thread(sel.clone(), path.to_path_buf());
        sel
    }

    pub fn search(&self, query: &str) -> Vec<(String, String)> {
        let query = query.replace(['-', '_'], "").to_lowercase();

        let res = self
            .data
            .iter()
            .filter(|(_, (search, ver))| search.starts_with(&query) && !ver.is_empty())
            .map(|(name, (_, version))| (name.to_string(), version.first().unwrap().to_string()))
            .collect::<Vec<_>>();

        res
    }

    pub fn get_versions(&self, name: &str) -> Option<Vec<String>> {
        let name = name.to_lowercase();
        self.data.get(&name).map(|(_, v)| v.clone())
    }
}

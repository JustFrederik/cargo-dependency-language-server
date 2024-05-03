use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crates_index::SparseIndex;
use update::{now, read_data, update_thread};

mod git;
mod update;

pub struct TomlData {
    last_checked: Duration,
    updating: bool,
    pub data: HashMap<String, (String, Vec<String>)>,
}
#[test]
fn test() {
    let index = crates_index::SparseIndex::new_cargo_default().unwrap();
    let info = index.crate_from_cache("").unwrap();
}

async fn update_info(index: &mut SparseIndex, krate: &str) {
    let req = index.make_cache_request(krate).unwrap().body(()).unwrap();

    let (parts, _) = req.into_parts();
    let req = http::Request::from_parts(parts, vec![]);

    let req: reqwest::Request = req.try_into().unwrap();

    let client = reqwest::ClientBuilder::new().gzip(true).build().unwrap();

    let res = client.execute(req).await.unwrap();

    let mut builder = http::Response::builder()
        .status(res.status())
        .version(res.version());

    builder
        .headers_mut()
        .unwrap()
        .extend(res.headers().iter().map(|(k, v)| (k.clone(), v.clone())));

    let body = res.bytes().await.unwrap().to_vec();
    let res = builder.body(body.to_vec()).unwrap();

    index.parse_cache_response(krate, res, true).unwrap();
}

impl TomlData {
    pub fn new(path: &Path) -> Arc<Mutex<Self>> {
        let data = read_data(path);
        let sel = Arc::new(Mutex::new(Self {
            last_checked: Duration::from_micros(0),
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

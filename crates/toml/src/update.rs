use std::{
    collections::HashMap,
    fs::{read_dir, read_to_string},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{git::git, TomlData};

fn update(toml_data: Arc<Mutex<TomlData>>, path: &Path) {
    {
        toml_data.lock().unwrap().updating = true;
    }

    let update = git(path);
    if !update {
        let mut lock = toml_data.lock().unwrap();
        lock.last_checked = now();
        lock.updating = false;
        return;
    }
    let data = read_data(path);
    let mut lock = toml_data.lock().unwrap();
    lock.data = data;
    lock.last_checked = now();
    lock.updating = false;
}

pub fn now() -> Duration {
    //TODO get wasm timestamp
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
}

pub fn update_thread(data: Arc<Mutex<TomlData>>, path: PathBuf) {
    //TODO: wasm thread
    thread::spawn(move || loop {
        let data = data.clone();
        let need_update = {
            let lock = data.lock().unwrap();
            match lock.updating {
                true => false,
                false => match lock.last_checked < now() - Duration::from_secs(300) {
                    true => true,
                    false => false,
                },
            }
        };
        if need_update {
            update(data, &path)
        }
        //TODO: wasm sleep
        sleep(Duration::from_secs(60))
    });
}

pub fn read_data(path: &Path) -> HashMap<String, (String, Vec<String>)> {
    let mut entries = HashMap::new();
    if let Ok(dir) = read_dir(path.join("index")) {
        for file in dir {
            let file = file.unwrap().path();
            let name = file
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if !name.ends_with(".json") || name.starts_with(".") {
                continue;
            }
            for (key, value) in read_to_string(file)
                .unwrap()
                .split("\n")
                .map(|line| serde_json::from_str::<(String, Vec<String>)>(line).unwrap())
                .map(|(key, value)| {
                    (
                        key.to_lowercase(),
                        (key.replace(['-', '_'], "").to_lowercase(), value),
                    )
                })
            {
                entries.insert(key, value);
            }
        }
    }

    entries
}

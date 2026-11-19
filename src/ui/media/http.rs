use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui::{
    self,
    load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError},
    mutex::Mutex,
};
use tokio::sync::oneshot::{self, error::TryRecvError};

struct ImageFile {
    bytes: Bytes,
    mime: Option<String>,
}

type FetchResult = Result<ImageFile, String>;

enum Cached {
    Pending(oneshot::Receiver<FetchResult>),
    Ready(FetchResult),
}

impl Cached {
    fn result(&mut self) -> Option<&FetchResult> {
        if let Self::Pending(receiver) = self {
            let result = match receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Closed) => Err("Image download was interrupted.".into()),
            };
            *self = Self::Ready(result);
        }
        match self {
            Self::Ready(result) => Some(result),
            Self::Pending(_) => None,
        }
    }
}

struct HttpImages {
    runtime: tokio::runtime::Handle,
    client: OnceLock<Result<reqwest::Client, String>>,
    cache: Mutex<HashMap<String, Cached>>,
}

pub(in crate::app) fn install(context: &egui::Context, runtime: &tokio::runtime::Handle) {
    context.add_bytes_loader(Arc::new(HttpImages {
        runtime: runtime.clone(),
        client: OnceLock::new(),
        cache: Mutex::default(),
    }));
}

impl BytesLoader for HttpImages {
    fn id(&self) -> &str {
        egui::generate_loader_id!(HttpImages)
    }

    fn load(&self, context: &egui::Context, uri: &str) -> BytesLoadResult {
        if !uri.starts_with("https://") && !uri.starts_with("http://") {
            return Err(LoadError::NotSupported);
        }

        let mut cache = self.cache.lock();
        if let Some(entry) = cache.get_mut(uri) {
            return match entry.result() {
                Some(Ok(file)) => Ok(BytesPoll::Ready {
                    size: None,
                    bytes: file.bytes.clone(),
                    mime: file.mime.clone(),
                }),
                Some(Err(error)) => Err(LoadError::Loading(error.clone())),
                None => Ok(BytesPoll::Pending { size: None }),
            };
        }

        let client = self.client.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|error| error.to_string())
        });
        let client = client
            .as_ref()
            .map_err(|error| LoadError::Loading(error.clone()))?;
        let (mut sender, receiver) = oneshot::channel();
        cache.insert(uri.to_owned(), Cached::Pending(receiver));
        let (context, uri, client) = (context.clone(), uri.to_owned(), client.clone());
        self.runtime.spawn(async move {
            tokio::select! {
                _ = sender.closed() => {}
                result = fetch(&client, &uri) => {
                    if sender.send(result).is_ok() {
                        context.request_repaint();
                    }
                }
            }
        });
        Ok(BytesPoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values_mut()
            .filter_map(Cached::result)
            .map(|result| match result {
                Ok(file) => file.bytes.len() + file.mime.as_ref().map_or(0, String::len),
                Err(error) => error.len(),
            })
            .sum()
    }

    fn has_pending(&self) -> bool {
        self.cache
            .lock()
            .values_mut()
            .any(|entry| entry.result().is_none())
    }
}

async fn fetch(client: &reqwest::Client, uri: &str) -> FetchResult {
    let response = client
        .get(uri)
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    Ok(ImageFile {
        bytes: Bytes::Shared(Arc::from(bytes.as_ref())),
        mime,
    })
}

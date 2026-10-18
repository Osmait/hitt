use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::auth::AuthConfig;
use super::chain::RequestChain;
use super::cookie_jar::CookieJar;
use super::request::{KeyValuePair, Request};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<CollectionItem>,
    pub variables: Vec<KeyValuePair>,
    pub auth: Option<AuthConfig>,
    #[serde(default)]
    pub cookie_jar: CookieJar,
    #[serde(default)]
    pub chains: Vec<RequestChain>,
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            items: Vec::new(),
            variables: Vec::new(),
            auth: None,
            cookie_jar: CookieJar::default(),
            chains: Vec::new(),
        }
    }

    pub fn add_request(&mut self, request: Request) {
        self.items.push(CollectionItem::Request(Box::new(request)));
    }

    pub fn add_folder(&mut self, name: impl Into<String>) -> &mut Vec<CollectionItem> {
        self.items.push(CollectionItem::Folder {
            id: Uuid::new_v4(),
            name: name.into(),
            items: Vec::new(),
            auth: None,
            description: None,
        });
        // SAFETY: we just pushed an item, so last_mut() is guaranteed to return Some
        match self.items.last_mut().unwrap() {
            CollectionItem::Folder { items, .. } => items,
            CollectionItem::Request(_) => unreachable!(),
        }
    }

    pub fn find_request(&self, id: &Uuid) -> Option<&Request> {
        Self::find_request_in_items(&self.items, id)
    }

    pub fn find_request_mut(&mut self, id: &Uuid) -> Option<&mut Request> {
        Self::find_request_mut_in_items(&mut self.items, id)
    }

    fn find_request_in_items<'a>(items: &'a [CollectionItem], id: &Uuid) -> Option<&'a Request> {
        for item in items {
            match item {
                CollectionItem::Request(req) if req.id == *id => return Some(req),
                CollectionItem::Folder { items, .. } => {
                    if let Some(found) = Self::find_request_in_items(items, id) {
                        return Some(found);
                    }
                }
                CollectionItem::Request(_) => {}
            }
        }
        None
    }

    fn find_request_mut_in_items<'a>(
        items: &'a mut [CollectionItem],
        id: &Uuid,
    ) -> Option<&'a mut Request> {
        for item in items {
            match item {
                CollectionItem::Request(req) if req.id == *id => return Some(req),
                CollectionItem::Folder { items, .. } => {
                    if let Some(found) = Self::find_request_mut_in_items(items, id) {
                        return Some(found);
                    }
                }
                CollectionItem::Request(_) => {}
            }
        }
        None
    }

    pub fn all_requests(&self) -> Vec<&Request> {
        let mut requests = Vec::new();
        Self::collect_requests(&self.items, &mut requests);
        requests
    }

    fn collect_requests<'a>(items: &'a [CollectionItem], out: &mut Vec<&'a Request>) {
        for item in items {
            match item {
                CollectionItem::Request(req) => out.push(req),
                CollectionItem::Folder { items, .. } => Self::collect_requests(items, out),
            }
        }
    }

    pub fn request_count(&self) -> usize {
        self.iter_requests().count()
    }

    /// Stack-based iterator over all requests in the collection.
    /// Avoids allocating a Vec unlike `all_requests()`.
    pub fn iter_requests(&self) -> RequestIter<'_> {
        RequestIter {
            stack: self.items.iter().rev().collect(),
        }
    }
}

/// Non-allocating iterator over all requests in a collection tree.
pub struct RequestIter<'a> {
    stack: Vec<&'a CollectionItem>,
}

impl<'a> Iterator for RequestIter<'a> {
    type Item = &'a Request;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.stack.pop() {
            match item {
                CollectionItem::Request(req) => return Some(req),
                CollectionItem::Folder { items, .. } => {
                    // Push children in reverse so they're visited in order
                    self.stack.extend(items.iter().rev());
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum CollectionItem {
    Request(Box<Request>),
    Folder {
        id: Uuid,
        name: String,
        items: Vec<CollectionItem>,
        auth: Option<AuthConfig>,
        description: Option<String>,
    },
}

impl CollectionItem {
    pub fn name(&self) -> &str {
        match self {
            Self::Request(r) => r.name.as_str(),
            Self::Folder { name, .. } => name,
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, Self::Folder { .. })
    }
}

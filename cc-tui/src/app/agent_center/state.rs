use std::time::Instant;

use cc_agent::messages::BaseMessage;
use chrono::{DateTime, Utc};
use tokio::sync::oneshot;

use super::{App, CenterPage, StoreResult};
use crate::thread::ThreadMeta;

type Pending<T> = Option<oneshot::Receiver<StoreResult<T>>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Bucket {
    NeedsYou,
    Working,
    Ready,
    Inactive,
}

impl Bucket {
    pub fn from_meta(meta: &ThreadMeta, now: DateTime<Utc>) -> Self {
        if meta.agent_status == "active"
            && now.signed_duration_since(meta.updated_at).num_hours() < 24
        {
            Self::Ready
        } else {
            Self::Inactive
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::Working => "Working",
            Self::Ready => "Ready",
            Self::Inactive => "Inactive",
        }
    }
    pub fn filter_index(self) -> usize {
        match self {
            Self::NeedsYou => 1,
            Self::Working => 2,
            Self::Ready => 3,
            Self::Inactive => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Grouping {
    #[default]
    Project,
    Status,
    Model,
}
impl Grouping {
    pub fn next(self) -> Self {
        match self {
            Self::Project => Self::Status,
            Self::Status => Self::Model,
            Self::Model => Self::Project,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Status => "Status",
            Self::Model => "Model",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CenterRow {
    Thread(usize),
    More,
}

pub(crate) struct Detail {
    pub prompt: String,
    pub last: String,
}
impl Detail {
    pub fn from_messages(messages: &[BaseMessage]) -> Self {
        Self {
            prompt: messages
                .iter()
                .find(|message| matches!(message, BaseMessage::Human { .. }))
                .map(BaseMessage::content)
                .unwrap_or_default(),
            last: messages
                .iter()
                .rev()
                .filter(|message| !message.is_system())
                .map(BaseMessage::content)
                .find(|text| !text.trim().is_empty())
                .unwrap_or_default(),
        }
    }
}

pub(crate) struct AgentCenter {
    pub threads: Vec<ThreadMeta>,
    pub catalog: Vec<ThreadMeta>,
    pub offset: usize,
    pub selected: usize,
    pub scroll: usize,
    pub filter: usize,
    pub grouping: Grouping,
    pub search: bool,
    pub query: String,
    pub rename: Option<String>,
    pub rename_id: Option<String>,
    pub confirm_delete: Option<String>,
    pub help: bool,
    pub error: Option<String>,
    pub page_rx: Pending<CenterPage>,
    pub mutation_rx: Pending<(String, Option<String>)>,
    pub detail_rx: Pending<(String, Vec<BaseMessage>)>,
    pub detail_id: Option<String>,
    pub detail: Option<Detail>,
    pub detail_due: Instant,
    pub last_counts: [usize; 5],
}
impl AgentCenter {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            catalog: Vec::new(),
            offset: 0,
            selected: 0,
            scroll: 0,
            filter: 0,
            grouping: Grouping::Project,
            search: false,
            query: String::new(),
            rename: None,
            rename_id: None,
            confirm_delete: None,
            help: false,
            error: None,
            page_rx: None,
            mutation_rx: None,
            detail_rx: None,
            detail_id: None,
            detail: None,
            detail_due: Instant::now(),
            last_counts: [0; 5],
        }
    }
    pub fn counts(&self, app: &App) -> [usize; 5] {
        let mut counts = [0; 5];
        for meta in self.catalog.iter().filter(|meta| meta.is_root()) {
            counts[0] += 1;
            counts[app.agent_center_bucket(meta).filter_index()] += 1;
        }
        counts
    }
    pub fn group_key(&self, app: &App, meta: &ThreadMeta) -> String {
        match self.grouping {
            Grouping::Project => meta.cwd.clone(),
            Grouping::Status => app.agent_center_bucket(meta).label().to_string(),
            Grouping::Model => meta.model.clone().unwrap_or_else(|| "Unknown".into()),
        }
    }
    pub fn rows(&self, app: &App) -> Vec<CenterRow> {
        let query = self.query.to_lowercase();
        let mut indices: Vec<_> = self
            .threads
            .iter()
            .enumerate()
            .filter(|(_, meta)| {
                (self.filter == 0 || app.agent_center_bucket(meta).filter_index() == self.filter)
                    && meta
                        .title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&query)
            })
            .map(|(index, _)| index)
            .collect();
        // 组的首次出现顺序沿用更新时间，组内也按更新时间排列。
        let mut groups: Vec<String> = Vec::new();
        for index in &indices {
            let key = self.group_key(app, &self.threads[*index]);
            if !groups.contains(&key) {
                groups.push(key);
            }
        }
        indices.sort_by_key(|index| {
            groups
                .iter()
                .position(|key| key == &self.group_key(app, &self.threads[*index]))
                .unwrap_or(0)
        });
        let mut rows: Vec<_> = indices.into_iter().map(CenterRow::Thread).collect();
        if self.offset < self.catalog.iter().filter(|meta| meta.is_root()).count() {
            rows.push(CenterRow::More);
        }
        rows
    }
    pub fn selected_meta<'a>(&'a self, app: &App) -> Option<&'a ThreadMeta> {
        match self.rows(app).get(self.selected)? {
            CenterRow::Thread(index) => self.threads.get(*index),
            CenterRow::More => None,
        }
    }
    pub fn reset_detail(&mut self) {
        self.detail_id = None;
        self.detail = None;
        self.detail_rx = None;
        self.detail_due = Instant::now();
    }
    pub fn refresh_catalog(&mut self, catalog: Vec<ThreadMeta>) {
        let previous_len = self.threads.len();
        self.threads.retain_mut(|meta| {
            if let Some(fresh) = catalog
                .iter()
                .find(|fresh| fresh.id == meta.id && fresh.is_root())
            {
                *meta = fresh.clone();
                true
            } else {
                false
            }
        });
        self.offset = self
            .offset
            .saturating_sub(previous_len - self.threads.len());
        self.catalog = catalog;
    }
    pub fn reset_selection(&mut self) {
        self.selected = 0;
        self.scroll = 0;
        self.reset_detail();
    }
}

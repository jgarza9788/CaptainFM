//! `DirModel`: a QAbstractListModel over one folder (or a deep-search result set).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QVector};
use qobject::*;

use crate::fuzzy::Query;
use crate::index::{self, Index};
use crate::listing::{self, Entry, Meta, UNKNOWN};
use crate::watch::{Watch, watch_many};

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!(< QAbstractListModel >);
        type QAbstractListModel;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvector.h");
        type QVector_i32 = cxx_qt_lib::QVector<i32>;
    }

    #[qenum(DirModel)]
    enum DirRoles {
        Name,
        IsDir,
        IsLink,
        Size,
        Mtime,
        FullPath,
        Rel,
        Selected,
        Hidden,
        Depth,
        Expanded,
        GroupName,
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[base = QAbstractListModel]
        #[qproperty(QString, path)]
        #[qproperty(QString, mode)]
        #[qproperty(bool, show_hidden, cxx_name = "showHidden")]
        #[qproperty(QString, sort_key, cxx_name = "sortKey")]
        #[qproperty(bool, sort_desc, cxx_name = "sortDesc")]
        #[qproperty(QString, filter)]
        #[qproperty(bool, loading)]
        #[qproperty(i32, search_ms, cxx_name = "searchMs")]
        #[qproperty(QString, group_by, cxx_name = "groupBy")]
        #[qproperty(bool, tree)]
        #[qproperty(bool, lite)]
        #[qproperty(i32, entry_count, cxx_name = "entryCount")]
        #[qproperty(i32, selected_count, cxx_name = "selectedCount")]
        /// Server the folder lives on ("" for a local folder).
        #[qproperty(QString, remote)]
        type DirModel = super::DirModelRust;

        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(&self, parent: &QModelIndex) -> i32;

        #[cxx_override]
        fn data(&self, index: &QModelIndex, role: i32) -> QVariant;

        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(&self) -> QHash_i32_QByteArray;

        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut Self>);

        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut Self>);

        #[inherit]
        fn index(self: &DirModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;

        #[inherit]
        #[qsignal]
        #[cxx_name = "dataChanged"]
        fn data_changed(
            self: Pin<&mut DirModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QVector_i32,
        );

        /// Show `path` (directories first) and watch it for changes.
        #[qinvokable]
        fn load(self: Pin<&mut Self>, path: &QString);

        /// Re-read the current folder.
        #[qinvokable]
        fn reload(self: Pin<&mut Self>);

        /// Navigate to the parent of the current path.
        #[qinvokable]
        fn up(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "index_of"]
        fn indexOf(&self, name: &QString) -> i32;

        /// First row at or after `from` (wrapping) whose name starts with `prefix`, ignoring case; -1 if none.
        #[qinvokable]
        #[rust_name = "find_prefix"]
        fn findPrefix(&self, prefix: &QString, from: i32) -> i32;

        #[qinvokable]
        #[rust_name = "row_of_path"]
        fn rowOfPath(&self, path: &QString) -> i32;

        #[qinvokable]
        #[rust_name = "path_at"]
        fn pathAt(&self, row: i32) -> QString;

        #[qinvokable]
        #[rust_name = "is_dir_at"]
        fn isDirAt(&self, row: i32) -> bool;

        #[qinvokable]
        #[rust_name = "set_hidden_visible"]
        fn setShowHiddenFiles(self: Pin<&mut Self>, show: bool);

        #[qinvokable]
        #[rust_name = "set_sort"]
        fn setSort(self: Pin<&mut Self>, key: &QString, desc: bool);

        #[qinvokable]
        #[rust_name = "set_filter_text"]
        fn setFilterText(self: Pin<&mut Self>, text: &QString);

        #[qinvokable]
        #[rust_name = "toggle_select"]
        fn toggleSelect(self: Pin<&mut Self>, row: i32);

        #[qinvokable]
        #[rust_name = "select_range"]
        fn selectRange(self: Pin<&mut Self>, from: i32, to: i32, on: bool);

        #[qinvokable]
        #[rust_name = "select_all"]
        fn selectAll(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "clear_selection"]
        fn clearSelection(self: Pin<&mut Self>);

        /// JSON array of the selected paths.
        #[qinvokable]
        #[rust_name = "selected_paths"]
        fn selectedPaths(&self) -> QString;

        /// Start a deep search over `root` (builds or reuses a drive-wide index).
        #[qinvokable]
        #[rust_name = "deep_start"]
        fn deepStart(self: Pin<&mut Self>, root: &QString);

        #[qinvokable]
        #[rust_name = "deep_query"]
        fn deepQuery(self: Pin<&mut Self>, query: &QString);

        /// Leave deep search and show the folder again.
        #[qinvokable]
        #[rust_name = "deep_stop"]
        fn deepStop(self: Pin<&mut Self>);

        /// Group rows under section headers: `none`, `type`, `date` or `size`.
        #[qinvokable]
        #[rust_name = "set_grouping"]
        fn applyGroupBy(self: Pin<&mut Self>, mode: &QString);

        /// Switch tree view (folders expand in place) on or off.
        #[qinvokable]
        #[rust_name = "set_tree_mode"]
        fn setTreeMode(self: Pin<&mut Self>, on: bool);

        /// Expand the folder at `row`; returns whether it changed anything.
        #[qinvokable]
        #[rust_name = "expand_row"]
        fn expandRow(self: Pin<&mut Self>, row: i32) -> bool;

        /// Collapse the folder at `row`; returns whether it changed anything.
        #[qinvokable]
        #[rust_name = "collapse_row"]
        fn collapseRow(self: Pin<&mut Self>, row: i32) -> bool;

        /// Row of the folder containing `row` in tree view, or -1 at the top level.
        #[qinvokable]
        #[rust_name = "parent_row"]
        fn parentRow(&self, row: i32) -> i32;

        /// Only show files with these extensions (JSON array; empty clears). Folders stay visible.
        #[qinvokable]
        #[rust_name = "set_ext_filter"]
        fn setExtFilter(self: Pin<&mut Self>, extensions: &QString);

        /// Emitted after the entries were replaced (navigation, reload, filter, sort).
        #[qsignal]
        fn loaded(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for DirModel {}
}

pub struct DirModelRust {
    path: QString,
    mode: QString,
    show_hidden: bool,
    sort_key: QString,
    sort_desc: bool,
    filter: QString,
    ext_filter: Vec<String>,
    loading: bool,
    search_ms: i32,
    group_by: QString,
    tree: bool,
    /// Names only: skip stat and folder-size passes (for preview columns).
    lite: bool,
    expanded: HashSet<String>,
    entry_count: i32,
    selected_count: i32,
    remote: QString,

    /// Everything read from the folder (or deep results), in read order. Shared with the worker
    /// thread that stats the entries, hence the `Arc`; sizes and times live in `meta`.
    all: Arc<Vec<Entry>>,
    meta: Vec<Meta>,
    /// Indices into `all` in display order (after hidden/filter/sort): what QML sees.
    order: Vec<u32>,
    selected: HashSet<String>,
    generation: Arc<AtomicU64>,
    watcher: Option<Watch>,
    deep_index: Option<Arc<Index>>,
    deep_query: String,
}

impl Default for DirModelRust {
    fn default() -> Self {
        Self {
            path: QString::default(),
            mode: QString::from("folder"),
            show_hidden: false,
            sort_key: QString::from("name"),
            sort_desc: false,
            filter: QString::default(),
            ext_filter: Vec::new(),
            loading: false,
            search_ms: 0,
            group_by: QString::from("none"),
            tree: false,
            lite: false,
            expanded: HashSet::new(),
            entry_count: 0,
            selected_count: 0,
            remote: QString::default(),
            all: Arc::new(Vec::new()),
            meta: Vec::new(),
            order: Vec::new(),
            selected: HashSet::new(),
            generation: Arc::new(AtomicU64::new(0)),
            watcher: None,
            deep_index: None,
            deep_query: String::new(),
        }
    }
}

/// How `all` is turned into the displayed rows.
struct ViewOpts<'a> {
    show_hidden: bool,
    filter: &'a str,
    /// Extension filter that spares folders (file picker).
    exts: &'a [String],
    key: &'a str,
    desc: bool,
    /// Set in tree view: the folder whose children are the top level.
    tree_root: Option<&'a str>,
    /// `type`, `date`, `size` or `none`: rows are ordered by group first.
    group: &'a str,
}

/// Display order for `all`: hidden files and filter applied, then sorted (or ranked, when the
/// filter has fuzzy terms).
fn compute_order(all: &[Entry], meta: &[Meta], opts: &ViewOpts) -> Vec<u32> {
    let query = Query::parse(opts.filter);
    let visible = |i: usize, e: &Entry| {
        (opts.show_hidden || !e.hidden)
            && (opts.exts.is_empty() || e.is_dir || crate::fuzzy::matches_ext(opts.exts, &e.name))
            && query.accepts(&e.name, e.is_dir, meta[i].size, meta[i].mtime)
    };
    if let (Some(root), false) = (opts.tree_root, query.has_terms()) {
        return listing::tree_order(all, meta, root, &visible, opts.key, opts.desc);
    }
    if opts.group != "none" && opts.tree_root.is_none() && !query.has_terms() {
        let now = crate::state::now();
        let mut idx: Vec<u32> = all
            .iter()
            .enumerate()
            .filter(|(i, e)| visible(*i, e))
            .map(|(i, _)| i as u32)
            .collect();
        listing::sort_indices(all, meta, &mut idx, opts.key, opts.desc);
        // Stable: within a group the chosen sort order is kept.
        idx.sort_by_key(|&i| {
            listing::group_of(&all[i as usize], meta[i as usize], opts.group, now).0
        });
        return idx;
    }
    if query.has_terms() {
        let mut scored: Vec<(i32, u32)> = all
            .iter()
            .enumerate()
            .filter(|(i, e)| visible(*i, e))
            .filter_map(|(i, e)| query.score(&e.name, &e.name).map(|s| (s, i as u32)))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| {
                listing::natural_cmp_ci(&all[a.1 as usize].lname, &all[b.1 as usize].lname)
            })
        });
        scored.into_iter().map(|(_, i)| i).collect()
    } else {
        let mut idx: Vec<u32> = all
            .iter()
            .enumerate()
            .filter(|(i, e)| visible(*i, e))
            .map(|(i, _)| i as u32)
            .collect();
        listing::sort_indices(all, meta, &mut idx, opts.key, opts.desc);
        idx
    }
}

/// First row from `from` (wrapping around) whose name starts with `prefix` (lower case), or -1.
fn find_prefix_in(all: &[Entry], order: &[u32], prefix: &str, from: i32) -> i32 {
    let n = order.len();
    if n == 0 {
        return -1;
    }
    let start = usize::try_from(from).unwrap_or(0) % n;
    (0..n)
        .map(|k| (start + k) % n)
        .find(|&row| all[order[row] as usize].lname.starts_with(prefix))
        .map_or(-1, |r| r as i32)
}

/// Row of the folder holding the entry at `row` (tree view), or -1 when it is not shown.
fn parent_row_in(all: &[Entry], order: &[u32], row: i32) -> i32 {
    let Some(e) = usize::try_from(row)
        .ok()
        .and_then(|r| order.get(r))
        .map(|&i| &all[i as usize])
    else {
        return -1;
    };
    let parent = e.path.rsplit_once('/').map_or("", |p| p.0);
    order
        .iter()
        .position(|&i| all[i as usize].path == parent)
        .map_or(-1, |i| i as i32)
}

/// Paths of the rows from `from` to `to` (either way round, clamped to the rows shown).
fn range_paths(all: &[Entry], order: &[u32], from: i32, to: i32) -> Vec<String> {
    let (lo, hi) = (from.min(to).max(0), from.max(to));
    order
        .iter()
        .enumerate()
        .filter(|(i, _)| (*i as i32) >= lo && (*i as i32) <= hi)
        .map(|(_, &i)| all[i as usize].path.clone())
        .collect()
}

/// Collapsing a folder also collapses every folder inside it.
fn collapse_in(expanded: &mut HashSet<String>, path: &str) {
    let prefix = format!("{path}/");
    expanded.retain(|p| p != path && !p.starts_with(&prefix));
}

impl DirModelRust {
    fn is_deep(&self) -> bool {
        self.mode.to_string() == "deep"
    }

    fn entry(&self, row: i32) -> Option<&Entry> {
        let i = *self.order.get(usize::try_from(row).ok()?)?;
        self.all.get(i as usize)
    }

    fn meta_at(&self, row: i32) -> Meta {
        self.order
            .get(usize::try_from(row).unwrap_or(usize::MAX))
            .and_then(|&i| self.meta.get(i as usize))
            .copied()
            .unwrap_or(Meta::UNKNOWN)
    }

    fn shown(&self) -> impl Iterator<Item = &Entry> {
        self.order.iter().map(|&i| &self.all[i as usize])
    }

    fn new_order(&self, all: &[Entry], meta: &[Meta]) -> Vec<u32> {
        if self.is_deep() {
            return (0..all.len() as u32).collect();
        }
        let (filter, key, root) = (
            self.filter.to_string(),
            self.sort_key.to_string(),
            self.path.to_string(),
        );
        let group = self.group_by.to_string();
        let opts = ViewOpts {
            show_hidden: self.show_hidden,
            filter: &filter,
            exts: &self.ext_filter,
            key: &key,
            desc: self.sort_desc,
            tree_root: self.tree.then_some(root.as_str()),
            group: &group,
        };
        compute_order(all, meta, &opts)
    }

    /// Whether sort or filter depend on size/mtime, which arrive after the names.
    fn order_depends_on_meta(&self) -> bool {
        !self.is_deep()
            && (matches!(self.sort_key.to_string().as_str(), "size" | "mtime")
                || matches!(self.group_by.to_string().as_str(), "date" | "size")
                || !self.filter.is_empty())
    }
}

impl qobject::DirModel {
    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.order.len() as i32
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let role = DirRoles { repr: role };
        let Some(e) = self.entry(index.row()) else {
            return QVariant::default();
        };
        let s = |v: &str| QVariant::from(&QString::from(v));
        match role {
            DirRoles::Name => s(&e.name),
            DirRoles::IsDir => QVariant::from(&e.is_dir),
            DirRoles::IsLink => QVariant::from(&e.is_link),
            DirRoles::Size => QVariant::from(&self.meta_at(index.row()).size),
            DirRoles::Mtime => QVariant::from(&self.meta_at(index.row()).mtime),
            DirRoles::FullPath => s(&e.path),
            DirRoles::Rel => s(&e.rel),
            DirRoles::Selected => QVariant::from(&self.selected.contains(&e.path)),
            DirRoles::Hidden => QVariant::from(&e.hidden),
            DirRoles::Depth => QVariant::from(&(e.depth as i32)),
            DirRoles::Expanded => QVariant::from(&self.expanded.contains(&e.path)),
            DirRoles::GroupName => {
                let group = self.group_by.to_string();
                let (_, label) =
                    listing::group_of(e, self.meta_at(index.row()), &group, crate::state::now());
                s(label)
            }
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> QHash_i32_QByteArray {
        let mut hash = QHash_i32_QByteArray::default();
        for (role, name) in [
            (DirRoles::Name, "name"),
            (DirRoles::IsDir, "isDir"),
            (DirRoles::IsLink, "isLink"),
            (DirRoles::Size, "size"),
            (DirRoles::Mtime, "mtime"),
            (DirRoles::FullPath, "fullPath"),
            (DirRoles::Rel, "rel"),
            (DirRoles::Selected, "selected"),
            (DirRoles::Hidden, "hidden"),
            (DirRoles::Depth, "depth"),
            (DirRoles::Expanded, "expanded"),
            (DirRoles::GroupName, "groupName"),
        ] {
            hash.insert(role.repr, name.into());
        }
        hash
    }

    // ---- navigation -------------------------------------------------------------------

    fn load(mut self: Pin<&mut Self>, path: &QString) {
        let dir = path.to_string();
        let dir = if dir.len() > 1 {
            dir.trim_end_matches('/').to_owned()
        } else {
            dir
        };
        let changed = self.path().to_string() != dir || self.is_deep();
        self.as_mut().set_mode(QString::from("folder"));
        self.as_mut().rust_mut().deep_index = None;
        self.as_mut().set_path(QString::from(dir.as_str()));
        if changed {
            // Show an empty list straight away instead of the previous folder's rows.
            self.as_mut().begin_reset_model();
            {
                let mut this = self.as_mut().rust_mut();
                this.selected.clear();
                this.expanded.clear();
                this.all = Arc::new(Vec::new());
                this.meta.clear();
                this.order.clear();
            }
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_filter(QString::default());
        self.as_mut().set_selected_count(0);
        self.as_mut()
            .set_remote(QString::from(listing::remote_label(&dir).as_str()));

        self.as_mut().rewatch();
        self.refresh(true);
    }

    /// Watch the folder (and, in tree view, every expanded sub-folder) for changes. Not on a remote:
    /// setting the watch up looks the path up here on the UI thread, which can wait seconds on a fresh
    /// mount, and inotify never sees changes made on the server anyway. Our own changes reload through `opDone`.
    fn rewatch(mut self: Pin<&mut Self>) {
        if !self.remote().is_empty() {
            self.as_mut().rust_mut().watcher = None;
            return;
        }
        let mut paths = vec![self.path().to_string()];
        paths.extend(self.expanded.iter().cloned());
        let qt_thread = self.qt_thread();
        let watcher = watch_many(&paths, false, Duration::from_millis(120), move || {
            let _ = qt_thread.queue(|model| model.refresh(false));
        });
        self.as_mut().rust_mut().watcher = watcher;
    }

    fn set_grouping(mut self: Pin<&mut Self>, mode: &QString) {
        self.as_mut().set_group_by(mode.clone());
        self.rederive();
    }

    fn set_tree_mode(mut self: Pin<&mut Self>, on: bool) {
        if *self.tree() == on {
            return;
        }
        self.as_mut().set_tree(on);
        self.as_mut().rust_mut().expanded.clear();
        if !self.is_deep() {
            self.as_mut().rewatch();
            self.refresh(true);
        }
    }

    fn expand_row(mut self: Pin<&mut Self>, row: i32) -> bool {
        let Some(e) = self.entry(row).filter(|e| e.is_dir && !e.is_link).cloned() else {
            return false;
        };
        if !*self.tree() || self.expanded.contains(&e.path) {
            return false;
        }
        self.as_mut().rust_mut().expanded.insert(e.path);
        self.as_mut().rewatch();
        self.refresh(true);
        true
    }

    fn collapse_row(mut self: Pin<&mut Self>, row: i32) -> bool {
        let Some(path) = self.entry(row).map(|e| e.path.clone()) else {
            return false;
        };
        if !self.expanded.contains(&path) {
            return false;
        }
        collapse_in(&mut self.as_mut().rust_mut().expanded, &path);
        self.as_mut().rewatch();
        self.refresh(true);
        true
    }

    fn parent_row(&self, row: i32) -> i32 {
        parent_row_in(&self.all, &self.order, row)
    }

    fn reload(self: Pin<&mut Self>) {
        self.refresh(true);
    }

    fn up(self: Pin<&mut Self>) {
        let current = self.path().to_string();
        if let Some(parent) = Path::new(&current).parent() {
            self.load(&QString::from(parent.to_string_lossy().as_ref()));
        }
    }

    fn index_of(&self, name: &QString) -> i32 {
        let name = name.to_string();
        self.shown()
            .position(|e| e.name == name)
            .map_or(-1, |i| i as i32)
    }

    fn find_prefix(&self, prefix: &QString, from: i32) -> i32 {
        find_prefix_in(
            &self.all,
            &self.order,
            &prefix.to_string().to_lowercase(),
            from,
        )
    }

    fn row_of_path(&self, path: &QString) -> i32 {
        let path = path.to_string();
        self.shown()
            .position(|e| e.path == path)
            .map_or(-1, |i| i as i32)
    }

    fn path_at(&self, row: i32) -> QString {
        self.entry(row)
            .map_or_else(QString::default, |e| QString::from(e.path.as_str()))
    }

    fn is_dir_at(&self, row: i32) -> bool {
        self.entry(row).is_some_and(|e| e.is_dir)
    }

    // ---- view options -----------------------------------------------------------------

    fn set_hidden_visible(mut self: Pin<&mut Self>, show: bool) {
        self.as_mut().set_show_hidden(show);
        self.rederive();
    }

    fn set_sort(mut self: Pin<&mut Self>, key: &QString, desc: bool) {
        self.as_mut().set_sort_key(key.clone());
        self.as_mut().set_sort_desc(desc);
        self.rederive();
    }

    fn set_ext_filter(mut self: Pin<&mut Self>, extensions: &QString) {
        let exts: Vec<String> = serde_json::from_str(&extensions.to_string()).unwrap_or_default();
        self.as_mut().rust_mut().ext_filter = exts
            .into_iter()
            .map(|e| e.trim_start_matches('.').to_lowercase())
            .collect();
        self.rederive();
    }

    fn set_filter_text(mut self: Pin<&mut Self>, text: &QString) {
        self.as_mut().set_filter(text.clone());
        self.rederive();
    }

    // ---- selection --------------------------------------------------------------------

    fn emit_selection_changed(mut self: Pin<&mut Self>, first: i32, last: i32) {
        let count = self.selected.len() as i32;
        self.as_mut().set_selected_count(count);
        if self.order.is_empty() || last < first {
            return;
        }
        let top = self.index(first, 0, &QModelIndex::default());
        let bottom = self.index(last, 0, &QModelIndex::default());
        let mut roles = QVector::<i32>::default();
        roles.append(DirRoles::Selected.repr);
        self.data_changed(&top, &bottom, &roles);
    }

    fn toggle_select(mut self: Pin<&mut Self>, row: i32) {
        let Some(path) = self.entry(row).map(|e| e.path.clone()) else {
            return;
        };
        let sel = &mut self.as_mut().rust_mut().selected;
        if !sel.remove(&path) {
            sel.insert(path);
        }
        self.emit_selection_changed(row, row);
    }

    fn select_range(mut self: Pin<&mut Self>, from: i32, to: i32, on: bool) {
        let (lo, hi) = (from.min(to).max(0), from.max(to));
        let paths = range_paths(&self.all, &self.order, from, to);
        let sel = &mut self.as_mut().rust_mut().selected;
        for p in paths {
            if on {
                sel.insert(p);
            } else {
                sel.remove(&p);
            }
        }
        let last = (self.order.len() as i32 - 1).min(hi);
        self.emit_selection_changed(lo, last);
    }

    fn select_all(mut self: Pin<&mut Self>) {
        let paths: Vec<String> = self.shown().map(|e| e.path.clone()).collect();
        self.as_mut().rust_mut().selected.extend(paths);
        let last = self.order.len() as i32 - 1;
        self.emit_selection_changed(0, last);
    }

    fn clear_selection(mut self: Pin<&mut Self>) {
        if self.selected.is_empty() {
            return;
        }
        self.as_mut().rust_mut().selected.clear();
        let last = self.order.len() as i32 - 1;
        self.emit_selection_changed(0, last);
    }

    fn selected_paths(&self) -> QString {
        // Keep display order so batch operations are predictable.
        let paths: Vec<&str> = self
            .shown()
            .filter(|e| self.selected.contains(&e.path))
            .map(|e| e.path.as_str())
            .collect();
        QString::from(serde_json::to_string(&paths).unwrap_or_default().as_str())
    }

    // ---- deep search ------------------------------------------------------------------

    fn deep_start(mut self: Pin<&mut Self>, root: &QString) {
        let root = root.to_string();
        self.as_mut().rust_mut().watcher = None;
        self.as_mut().set_mode(QString::from("deep"));
        self.as_mut().set_path(QString::from(root.as_str()));
        self.as_mut().rust_mut().selected.clear();
        self.as_mut().set_selected_count(0);

        let qt_thread = self.qt_thread();
        let index = index::get_or_build(&root, move || {
            let _ = qt_thread.queue(|model| model.deep_refresh());
        });
        self.as_mut().rust_mut().deep_index = Some(index);
        self.as_mut().rust_mut().deep_query = String::new();
        self.deep_refresh();
    }

    fn deep_query(mut self: Pin<&mut Self>, query: &QString) {
        self.as_mut().rust_mut().deep_query = query.to_string();
        self.deep_refresh();
    }

    fn deep_stop(mut self: Pin<&mut Self>) {
        let path = self.path().clone();
        self.as_mut()
            .rust_mut()
            .generation
            .fetch_add(1, Ordering::SeqCst);
        self.load(&path);
    }

    /// Re-run the current deep query against the (possibly still growing) index.
    fn deep_refresh(mut self: Pin<&mut Self>) {
        let Some(index) = self.deep_index.clone().filter(|_| self.is_deep()) else {
            return;
        };
        let generation = Arc::clone(&self.generation);
        let ticket = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let query = self.deep_query.clone();
        let show_hidden = *self.show_hidden();
        let qt_thread = self.qt_thread();
        self.as_mut()
            .set_loading(!index.done.load(Ordering::SeqCst));

        std::thread::spawn(move || {
            let cancel_gen = Arc::clone(&generation);
            let t0 = std::time::Instant::now();
            let (results, metas) = index::search(&index, &query, show_hidden, &move || {
                cancel_gen.load(Ordering::SeqCst) != ticket
            });
            let done = index.done.load(Ordering::SeqCst);
            let ms = t0.elapsed().as_millis() as i32;
            let _ = qt_thread.queue(move |mut model| {
                if generation.load(Ordering::SeqCst) != ticket {
                    return;
                }
                model.as_mut().set_search_ms(ms);
                model.as_mut().set_loading(!done);
                let order: Vec<u32> = (0..results.len() as u32).collect();
                let same_rows = model.order.len() == order.len()
                    && model
                        .order
                        .iter()
                        .zip(&order)
                        .all(|(&a, &b)| model.all[a as usize].path == results[b as usize].path);
                {
                    let mut this = model.as_mut().rust_mut();
                    this.all = Arc::new(results);
                    this.meta = metas;
                }
                model.install_order(order, same_rows, false);
            });
        });
    }

    // ---- folder reads -----------------------------------------------------------------

    /// Read the folder on a worker thread: names first (instant paint), then file metadata,
    /// then recursive folder sizes. Each stage is applied on the Qt thread.
    fn refresh(mut self: Pin<&mut Self>, force: bool) {
        if self.is_deep() {
            return;
        }
        let path = self.path().to_string();
        let (tree, expanded, lite) = (*self.tree(), self.expanded.clone(), *self.lite());
        let remote = !self.remote().is_empty();
        let generation = Arc::clone(&self.generation);
        let ticket = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let qt_thread = self.qt_thread();
        self.as_mut().set_loading(true);

        std::thread::spawn(move || {
            let stale = {
                let g = Arc::clone(&generation);
                move || g.load(Ordering::SeqCst) != ticket
            };
            let t0 = std::time::Instant::now();
            let entries = Arc::new(if tree {
                listing::list_tree(&path, &expanded)
            } else {
                listing::list_dir(&path)
            });
            if std::env::var_os("CFM_DEBUG").is_some() {
                eprintln!(
                    "[timing] list_dir {} entries: {:?}",
                    entries.len(),
                    t0.elapsed()
                );
            }

            let (g, shared) = (Arc::clone(&generation), Arc::clone(&entries));
            let _ = qt_thread.queue(move |model| {
                if g.load(Ordering::SeqCst) == ticket {
                    model.apply_listing(shared, force);
                }
            });

            if lite {
                let g = Arc::clone(&generation);
                let _ = qt_thread.queue(move |mut model| {
                    if g.load(Ordering::SeqCst) == ticket {
                        model.as_mut().set_loading(false);
                    }
                });
                return;
            }

            // Stage 2: stat everything in chunks.
            const CHUNK: usize = 4096;
            let chunks = entries.len().div_ceil(CHUNK);
            for (chunk_no, chunk) in entries.chunks(CHUNK).enumerate() {
                if stale() {
                    return;
                }
                let base = chunk_no * CHUNK;
                let meta: Vec<(usize, i64, i64)> = chunk
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let (size, mtime) = listing::stat(&e.path);
                        // Directory sizes come from stage 3; keep any value already known.
                        (base + i, if e.is_dir { UNKNOWN - 1 } else { size }, mtime)
                    })
                    .collect();
                let last = chunk_no + 1 == chunks;
                let g = Arc::clone(&generation);
                let _ = qt_thread.queue(move |model| {
                    if g.load(Ordering::SeqCst) == ticket {
                        model.apply_meta(meta, last);
                    }
                });
            }

            // Stage 3: recursive folder sizes. Not on a remote: walking the whole tree there keeps the
            // connection busy and every other lookup queues behind it; folders show no size instead.
            if remote {
                let g = Arc::clone(&generation);
                let _ = qt_thread.queue(move |mut model| {
                    if g.load(Ordering::SeqCst) == ticket {
                        model.as_mut().set_loading(false);
                    }
                });
                return;
            }
            let dirs: Vec<(usize, String)> = entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.is_dir && !e.is_link)
                .map(|(i, e)| (i, e.path.clone()))
                .collect();
            let (g, qt) = (Arc::clone(&generation), qt_thread.clone());
            listing::dir_sizes_parallel(dirs, Duration::from_millis(200), &stale, move |batch| {
                let g = Arc::clone(&g);
                let _ = qt.queue(move |model| {
                    if g.load(Ordering::SeqCst) == ticket {
                        model.apply_dir_sizes(batch);
                    }
                });
            });
            let g = Arc::clone(&generation);
            let _ = qt_thread.queue(move |mut model| {
                if g.load(Ordering::SeqCst) == ticket {
                    model.as_mut().set_loading(false);
                }
            });
        });
    }

    /// Stage 1 result: names and types. Keeps sizes/mtimes already known for unchanged paths.
    fn apply_listing(mut self: Pin<&mut Self>, entries: Arc<Vec<Entry>>, force: bool) {
        let mut meta = vec![Meta::UNKNOWN; entries.len()];
        if !self.all.is_empty() {
            let known: HashMap<&str, Meta> = self
                .all
                .iter()
                .zip(&self.meta)
                .map(|(e, m)| (e.path.as_str(), *m))
                .collect();
            for (e, m) in entries.iter().zip(meta.iter_mut()) {
                if let Some(&old) = known.get(e.path.as_str()) {
                    *m = old;
                }
            }
        }
        let t0 = std::time::Instant::now();
        let order = self.new_order(&entries, &meta);
        if std::env::var_os("CFM_DEBUG").is_some() {
            eprintln!("[timing] new_order: {:?}", t0.elapsed());
        }
        let same_rows = self.order.len() == order.len()
            && self
                .order
                .iter()
                .zip(&order)
                .all(|(&old, &new)| self.all[old as usize].path == entries[new as usize].path);

        {
            let mut this = self.as_mut().rust_mut();
            this.all = entries;
            this.meta = meta;
        }
        self.install_order(order, same_rows, force);
    }

    /// `(index in all, size, mtime)`; a size below `UNKNOWN` means "leave the size alone".
    fn apply_meta(mut self: Pin<&mut Self>, updates: Vec<(usize, i64, i64)>, last: bool) {
        {
            let meta = &mut self.as_mut().rust_mut().meta;
            for (i, size, mtime) in updates {
                if let Some(m) = meta.get_mut(i) {
                    if size >= UNKNOWN {
                        m.size = size;
                    }
                    m.mtime = mtime;
                }
            }
        }
        // Re-sorting 100k rows per chunk would stall the UI, so wait for the final chunk.
        if last && self.order_depends_on_meta() {
            self.rederive();
        } else {
            self.cells_changed();
        }
    }

    fn apply_dir_sizes(mut self: Pin<&mut Self>, sizes: Vec<(usize, i64)>) {
        {
            let meta = &mut self.as_mut().rust_mut().meta;
            for (i, size) in sizes {
                if let Some(m) = meta.get_mut(i) {
                    m.size = size;
                }
            }
        }
        if self.order_depends_on_meta() {
            self.rederive();
        } else {
            self.cells_changed();
        }
    }

    /// Row contents changed but not the row order: refresh what is on screen.
    fn cells_changed(mut self: Pin<&mut Self>) {
        let last = self.order.len() as i32 - 1;
        if last < 0 {
            return;
        }
        let top = self.index(0, 0, &QModelIndex::default());
        let bottom = self.index(last, 0, &QModelIndex::default());
        self.as_mut()
            .data_changed(&top, &bottom, &QVector::<i32>::default());
    }

    /// Recompute the display order from `all` after a view option changed.
    fn rederive(self: Pin<&mut Self>) {
        let order = self.new_order(&self.all, &self.meta);
        let same_rows = order == self.order;
        self.install_order(order, same_rows, false);
    }

    /// Adopt `order`. Same rows in the same order: refresh cells in place (no flicker, the cursor
    /// stays); otherwise reset the model.
    fn install_order(
        mut self: Pin<&mut Self>,
        order: Vec<u32>,
        same_rows: bool,
        force_signal: bool,
    ) {
        if same_rows {
            self.as_mut().rust_mut().order = order;
            self.as_mut().cells_changed();
        } else {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().order = order;
            self.as_mut().end_reset_model();
            // Drop selections that no longer exist.
            let present: HashSet<&str> = self.all.iter().map(|e| e.path.as_str()).collect();
            let kept: HashSet<String> = self
                .selected
                .iter()
                .filter(|p| present.contains(p.as_str()))
                .cloned()
                .collect();
            self.as_mut().rust_mut().selected = kept;
        }
        let (count, selected) = (self.order.len() as i32, self.selected.len() as i32);
        self.as_mut().set_entry_count(count);
        self.as_mut().set_selected_count(selected);
        if !same_rows || force_signal {
            self.loaded();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entries under `/r`: a path ending in `/` is a folder, and a name starting with `.` is hidden.
    fn entries(paths: &[&str]) -> Vec<Entry> {
        paths
            .iter()
            .map(|p| {
                let is_dir = p.ends_with('/');
                let path = p.trim_end_matches('/');
                let name = path.rsplit('/').next().unwrap();
                Entry {
                    name: name.into(),
                    lname: name.to_lowercase(),
                    path: format!("/r/{path}"),
                    rel: String::new(),
                    is_dir,
                    is_link: false,
                    hidden: name.starts_with('.'),
                    depth: path.matches('/').count() as u16,
                }
            })
            .collect()
    }

    fn opts<'a>(filter: &'a str, key: &'a str, desc: bool) -> ViewOpts<'a> {
        ViewOpts {
            show_hidden: false,
            filter,
            exts: &[],
            key,
            desc,
            tree_root: None,
            group: "none",
        }
    }

    fn names(all: &[Entry], order: &[u32]) -> Vec<String> {
        order
            .iter()
            .map(|&i| all[i as usize].name.clone())
            .collect()
    }

    #[test]
    fn order_hides_dotfiles_and_puts_folders_first() {
        let all = entries(&["b.txt", ".hidden", "a.txt", "zdir/", "10.txt", "9.txt"]);
        let meta = vec![Meta::UNKNOWN; all.len()];
        let order = compute_order(&all, &meta, &opts("", "name", false));
        assert_eq!(
            names(&all, &order),
            ["zdir", "9.txt", "10.txt", "a.txt", "b.txt"]
        );
        let shown = ViewOpts {
            show_hidden: true,
            ..opts("", "name", false)
        };
        assert!(names(&all, &compute_order(&all, &meta, &shown)).contains(&".hidden".into()));
    }

    #[test]
    fn order_by_size_descending() {
        let all = entries(&["small", "big", "mid"]);
        let meta: Vec<Meta> = [1, 300, 20]
            .iter()
            .map(|&size| Meta { size, mtime: 0 })
            .collect();
        let order = compute_order(&all, &meta, &opts("", "size", true));
        assert_eq!(names(&all, &order), ["big", "mid", "small"]);
    }

    #[test]
    fn extension_filter_spares_folders() {
        let all = entries(&["a.pdf", "b.txt", "docs/", "c.PDF"]);
        let meta = vec![Meta::UNKNOWN; all.len()];
        let exts = ["pdf".to_string()];
        let o = ViewOpts {
            exts: &exts,
            ..opts("", "name", false)
        };
        assert_eq!(
            names(&all, &compute_order(&all, &meta, &o)),
            ["docs", "a.pdf", "c.PDF"]
        );
    }

    #[test]
    fn filter_text_keeps_matches_and_ties_go_by_name() {
        let all = entries(&["rep2.txt", "notes.md", "rep1.txt"]);
        let meta = vec![Meta::UNKNOWN; all.len()];
        let order = compute_order(&all, &meta, &opts("rep", "name", false));
        assert_eq!(names(&all, &order), ["rep1.txt", "rep2.txt"]);
    }

    #[test]
    fn grouping_keeps_the_sort_inside_each_group() {
        let all = entries(&["b.png", "z.rs", "a.png", "dir/"]);
        let meta = vec![Meta::UNKNOWN; all.len()];
        let o = ViewOpts {
            group: "type",
            ..opts("", "name", true)
        };
        // Folders, then images (descending by name), then code.
        assert_eq!(
            names(&all, &compute_order(&all, &meta, &o)),
            ["dir", "b.png", "a.png", "z.rs"]
        );
    }

    #[test]
    fn tree_order_nests_children_under_their_folder() {
        let all = entries(&["b/", "a/", "a/y.txt", "a/x.txt", "c.txt"]);
        let meta = vec![Meta::UNKNOWN; all.len()];
        let o = ViewOpts {
            tree_root: Some("/r"),
            ..opts("", "name", false)
        };
        let order = compute_order(&all, &meta, &o);
        assert_eq!(names(&all, &order), ["a", "x.txt", "y.txt", "b", "c.txt"]);
        // The parent of a nested row is its folder's row; top-level rows have none.
        assert_eq!(parent_row_in(&all, &order, 2), 0);
        assert_eq!(parent_row_in(&all, &order, 3), -1);
        assert_eq!(parent_row_in(&all, &order, 99), -1);
    }

    #[test]
    fn find_prefix_wraps_around_from_the_cursor() {
        let all = entries(&["apple", "banana", "avocado"]);
        let order = vec![0, 1, 2];
        assert_eq!(find_prefix_in(&all, &order, "a", 0), 0);
        assert_eq!(find_prefix_in(&all, &order, "a", 1), 2);
        assert_eq!(
            find_prefix_in(&all, &order, "a", 3),
            0,
            "past the end wraps to the top"
        );
        assert_eq!(find_prefix_in(&all, &order, "q", 0), -1);
        assert_eq!(find_prefix_in(&[], &[], "a", 0), -1);
    }

    #[test]
    fn range_selection_works_either_way_and_clamps() {
        let all = entries(&["a", "b", "c", "d"]);
        let order = vec![3, 2, 1, 0];
        assert_eq!(range_paths(&all, &order, 2, 1), ["/r/c", "/r/b"]);
        assert_eq!(range_paths(&all, &order, -5, 0), ["/r/d"]);
        assert_eq!(range_paths(&all, &order, 3, 99), ["/r/a"]);
    }

    #[test]
    fn collapsing_a_folder_collapses_the_folders_inside_it() {
        let mut expanded: HashSet<String> = ["/r/a", "/r/a/b", "/r/a/b/c", "/r/ab", "/r/z"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        collapse_in(&mut expanded, "/r/a");
        let mut left: Vec<&str> = expanded.iter().map(String::as_str).collect();
        left.sort();
        // `/r/ab` only shares the start of the name, so it stays open.
        assert_eq!(left, ["/r/ab", "/r/z"]);
    }
}

//! A SQLite database, browsed like a folder: the database itself lists its
//! tables and views, and each of those shows its rows a page at a time.
//!
//! Navigation is ordinary path navigation - `app.db` is a place, and
//! `app.db\customers` is a place inside it (see `core::sqlite::split`) - so
//! the breadcrumb, Back/Forward/Up, tabs and split panes all work here with no
//! knowledge of databases at all. Only the *drawing* is special, the same way
//! the Settings tab's is.
//!
//! Everything shown is read-only; `core::sqlite` cannot write, and this module
//! offers no gesture that would try.

use crate::core::sqlite::{self, Cell, ObjectInfo, ObjectKind, Page, RowCount};
use crate::gui::i18n::I18n;
use crate::gui::theme::ThemePalette;
use crate::gui::windows::containers::structs::ItemViewerNavBarAction;
use crossbeam_channel::{Receiver, unbounded};
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::path::{Path, PathBuf};

const ROW_HEIGHT: f32 = 22.0;
const HEADER_PADDING: f32 = 4.0;
/// Same left inset the folder and tag tables use, so every grid in the app
/// starts its content at the same place.
const LEFT_MARGIN: f32 = 8.0;
const MIN_COLUMN: f32 = 48.0;
const MAX_COLUMN: f32 = 420.0;
/// Rows measured when fitting columns to content. The rest of the page is
/// assumed to be similar - measuring five hundred rows to pick a width costs
/// more than the occasional column that ends up a little narrow.
const MEASURED_ROWS: usize = 40;

/// What a load produced, handed back from the worker thread.
enum Loaded {
    Objects(Result<Vec<ObjectInfo>, String>),
    Rows {
        page: Result<Page, String>,
        /// Only recounted when the object changed - see `Request::total`.
        total: Option<RowCount>,
    },
}

/// A database file and, when the path reaches inside it, the table or view
/// named there.
pub type DatabaseAndObject = (PathBuf, Option<String>);

/// What is being displayed, and what it was asked for.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Request {
    database: PathBuf,
    object: Option<String>,
    offset: usize,
}

/// One tab's view of one database.
///
/// The invariant that keeps this off the hot path: a query only ever starts
/// when `showing` differs from what the current path asks for. Drawing a frame
/// never opens the database.
#[derive(Default)]
pub struct SqliteState {
    /// What `objects`/`page` currently hold.
    showing: Option<Request>,
    /// What a worker is fetching right now, if any.
    pending: Option<Request>,
    rx: Option<Receiver<Loaded>>,
    objects: Vec<ObjectInfo>,
    page: Page,
    total: Option<RowCount>,
    /// Which object `total` was counted for, so paging doesn't recount.
    counted: Option<(PathBuf, String)>,
    offset: usize,
    error: Option<String>,
    /// Content-fitted on each load and never persisted. A database's columns
    /// are whatever that table happens to have, so there is no stable slot to
    /// save a width into - and a supplementary view writing into another
    /// view's saved column widths is a bug this codebase has already had once
    /// (see `compute_item_viewer_column_layout`'s search-view special case).
    widths: Vec<f32>,
    /// Folded into the table's `id_salt` so egui re-reads `Column::initial`
    /// after a fit, which it otherwise only consults once per table id.
    generation: u64,
    /// The last path `resolve` was asked about and what it decided.
    ///
    /// `sqlite::split` has to call `is_file` to tell a database apart from
    /// a folder that merely ends in `.db`, and the answer is needed by
    /// drawing code that runs every frame - so without this the app would
    /// stat the file sixty times a second for an answer that only changes
    /// when the tab navigates.
    resolved: Option<(PathBuf, Option<DatabaseAndObject>)>,
}

impl SqliteState {
    /// Drops everything held for a database - called when a tab navigates
    /// away, so a tab parked elsewhere isn't holding a page of rows.
    fn forget(&mut self) {
        self.showing = None;
        self.pending = None;
        self.rx = None;
        self.objects.clear();
        self.page = Page::default();
        self.total = None;
        self.counted = None;
        self.offset = 0;
        self.error = None;
        self.widths.clear();
        // `resolved` is deliberately kept: it is about the path, not the
        // contents, and throwing it away is what would reintroduce the
        // per-frame stat this exists to avoid.
    }
}

/// Draws the database at `database`, or the object inside it when `object` is
/// set. Navigation happens through `tabbar_action`, like every other view.
pub fn draw_sqlite_view(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut SqliteState,
    database: &Path,
    object: Option<&str>,
    // Settings > Behavior's own navigate-up switch: the same gesture here as
    // in an ordinary folder, so it answers to the same setting.
    double_click_navigates_up: bool,
    tabbar_action: &mut Option<ItemViewerNavBarAction>,
) {
    // Switching objects starts at the top rather than keeping the previous
    // object's scroll position, which would belong to a different row count.
    let switched = state
        .showing
        .as_ref()
        .or(state.pending.as_ref())
        .is_some_and(|r| {
            r.database != database || r.object.as_deref() != object
        });
    if switched {
        state.offset = 0;
    }

    poll(state);
    ensure_loaded(state, database, object, ui.ctx());

    if let Some(error) = state.error.clone() {
        draw_message(ui, palette, &error, true);
        return;
    }
    let ready = state.showing.as_ref().is_some_and(|r| {
        r.database == database && r.object.as_deref() == object
    });
    if !ready {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.add(egui::Spinner::new().size(28.0));
        });
        return;
    }

    match object {
        None => draw_object_list(
            ui,
            i18n,
            palette,
            state,
            database,
            double_click_navigates_up,
            tabbar_action,
        ),
        Some(object) => draw_rows(
            ui,
            i18n,
            palette,
            state,
            database,
            object,
            double_click_navigates_up,
            tabbar_action,
        ),
    }
}

/// `ItemViewerNavAction::Up` rather than a computed parent path: a database
/// path is an ordinary absolute path, so `Navigation::go_up` already walks it
/// correctly (a table to its database, the database to its folder), and going
/// through the same action keeps the history and selection handling that an
/// ordinary folder's Up gets.
fn go_up(tabbar_action: &mut Option<ItemViewerNavBarAction>) {
    tabbar_action.get_or_insert_with(Default::default).nav =
        Some(crate::gui::windows::containers::enums::ItemViewerNavAction::Up);
}

/// Whether `path` reaches into a SQLite database, and if so which one.
///
/// Memoized per tab because the caller is per-frame drawing code - see
/// `SqliteState::resolved`.
pub fn resolve(state: &mut SqliteState, path: &Path) -> Option<DatabaseAndObject> {
    if let Some((cached_path, result)) = state.resolved.as_ref()
        && cached_path == path
    {
        return result.clone();
    }
    let result = sqlite::split(path);
    state.resolved = Some((path.to_path_buf(), result.clone()));
    result
}

/// Called when a tab is showing something that isn't a database, so a tab
/// that navigated away stops holding rows in memory.
pub fn release(state: &mut SqliteState) {
    if state.showing.is_some() || state.pending.is_some() {
        state.forget();
    }
}

fn poll(state: &mut SqliteState) {
    let Some(rx) = state.rx.as_ref() else {
        return;
    };
    let Ok(loaded) = rx.try_recv() else {
        return;
    };
    state.rx = None;
    let Some(request) = state.pending.take() else {
        return;
    };

    match loaded {
        Loaded::Objects(Ok(objects)) => {
            state.objects = objects;
            state.error = None;
        }
        Loaded::Rows { page, total } => match page {
            Ok(page) => {
                if let Some(total) = total {
                    state.total = Some(total);
                    state.counted = request
                        .object
                        .clone()
                        .map(|o| (request.database.clone(), o));
                }
                // Fitted on the next frame, where there is a `Ui` to
                // measure with.
                state.widths.clear();
                state.page = page;
                state.error = None;
            }
            Err(error) => state.error = Some(error),
        },
        Loaded::Objects(Err(error)) => state.error = Some(error),
    }
    state.showing = Some(request);
}

fn ensure_loaded(
    state: &mut SqliteState,
    database: &Path,
    object: Option<&str>,
    ctx: &egui::Context,
) {
    let want = Request {
        database: database.to_path_buf(),
        object: object.map(str::to_string),
        offset: state.offset,
    };
    if state.showing.as_ref() == Some(&want) || state.pending.as_ref() == Some(&want) {
        return;
    }

    // A different request than the one in flight: let the old one land and be
    // discarded rather than trying to cancel it. Each query is short, and the
    // connection closes with it.
    state.error = None;
    state.pending = Some(want.clone());

    let (tx, rx) = unbounded();
    state.rx = Some(rx);
    let repaint = ctx.clone();

    // Counting is only worth doing when the object changed; paging within one
    // object keeps the count it already has.
    let needs_count = want.object.as_ref().is_some_and(|o| {
        state.counted.as_ref() != Some(&(want.database.clone(), o.clone()))
    });
    if needs_count {
        state.total = None;
    }

    std::thread::spawn(move || {
        let loaded = match want.object.as_deref() {
            None => Loaded::Objects(sqlite::objects(&want.database)),
            Some(object) => {
                let total = needs_count
                    .then(|| sqlite::count(&want.database, object).ok())
                    .flatten();
                Loaded::Rows {
                    page: sqlite::page(&want.database, object, want.offset, sqlite::PAGE_ROWS),
                    total,
                }
            }
        };
        let _ = tx.send(loaded);
        repaint.request_repaint();
    });
}

fn draw_message(ui: &mut egui::Ui, palette: &ThemePalette, text: &str, is_error: bool) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        let color = if is_error {
            ui.visuals().warn_fg_color
        } else {
            palette.text_header_section
        };
        ui.add(
            egui::Label::new(
                egui::RichText::new(text)
                    .size(palette.text_size)
                    .color(color),
            )
            .selectable(false),
        );
    });
}

fn draw_object_list(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut SqliteState,
    database: &Path,
    double_click_navigates_up: bool,
    tabbar_action: &mut Option<ItemViewerNavBarAction>,
) {
    if state.objects.is_empty() {
        draw_message(ui, palette, &i18n.tr("sqlite_no_tables"), false);
        return;
    }

    let objects = state.objects.clone();
    let table_rect = inset(ui.available_rect_before_wrap());

    let up = double_click_navigates_up
        && background_double_click(ui, table_rect, "sqlite_objects_bg");
    if up {
        go_up(tabbar_action);
    }

    ui.scope_builder(egui::UiBuilder::new().max_rect(table_rect), |ui| {
        ui.push_id(egui::Id::new(("sqlite_objects", database)), |ui| {
            TableBuilder::new(ui)
                .striped(false)
                .resizable(true)
                .vscroll(true)
                .sense(egui::Sense::click())
                .column(Column::initial(320.0).at_least(MIN_COLUMN).resizable(true))
                .column(Column::initial(100.0).at_least(MIN_COLUMN).resizable(true))
                .column(Column::remainder().at_least(MIN_COLUMN).resizable(false))
                .header(ROW_HEIGHT + HEADER_PADDING * 2.0, |mut header| {
                    for key in ["explorer_cols_name", "explorer_cols_type", "sqlite_rows"] {
                        header.col(|ui| {
                            ui.add_space(HEADER_PADDING);
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(i18n.tr(key))
                                        .size(palette.text_size)
                                        .color(palette.text_header_section),
                                )
                                .selectable(false),
                            );
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_HEIGHT, objects.len(), |mut row| {
                        let object = &objects[row.index()];
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(egui_phosphor::regular::TABLE)
                                            .size(palette.text_size)
                                            .color(palette.primary),
                                    )
                                    .selectable(false),
                                );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&object.name).size(palette.text_size),
                                    )
                                    .selectable(false),
                                );
                            });
                        });
                        row.col(|ui| {
                            let key = match object.kind {
                                ObjectKind::Table => "sqlite_kind_table",
                                ObjectKind::View => "sqlite_kind_view",
                            };
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(i18n.tr(key)).size(palette.text_size),
                                )
                                .selectable(false),
                            );
                        });
                        row.col(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(row_count_text(object.rows))
                                        .size(palette.text_size),
                                )
                                .selectable(false),
                            );
                        });

                        let resp = row.response();
                        if resp.double_clicked() {
                            tabbar_action
                                .get_or_insert_with(Default::default)
                                .nav_to = Some(database.join(&object.name));
                        }
                    });
                });
        });
    });
}

fn row_count_text(rows: Option<RowCount>) -> String {
    match rows {
        Some(count) if count.capped => format!("{}+", count.value),
        Some(count) => count.value.to_string(),
        None => "—".to_string(),
    }
}

fn draw_rows(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut SqliteState,
    database: &Path,
    object: &str,
    double_click_navigates_up: bool,
    tabbar_action: &mut Option<ItemViewerNavBarAction>,
) {
    if state.page.columns.is_empty() {
        // Defensive only: a loaded object always reports its columns, even
        // when it holds no rows (an empty table draws its headers and an
        // empty body, which is the right thing to show).
        return;
    }

    let footer_height = ROW_HEIGHT + 12.0;
    let full = ui.available_rect_before_wrap();
    let grid_rect = egui::Rect::from_min_max(
        full.min,
        egui::pos2(full.right(), (full.bottom() - footer_height).max(full.top())),
    );
    let footer_rect = egui::Rect::from_min_max(
        egui::pos2(full.left(), grid_rect.bottom()),
        full.right_bottom(),
    );

    if state.widths.len() != state.page.columns.len() {
        state.widths = fit_columns(ui, &state.page, palette.text_size);
        // egui only consults `Column::initial` the first time it sees a table
        // id, so a fresh fit needs a fresh id to be read at all.
        state.generation = state.generation.wrapping_add(1);
    }

    if double_click_navigates_up
        && background_double_click(ui, grid_rect, "sqlite_rows_bg")
    {
        go_up(tabbar_action);
    }

    let page = state.page.clone();
    let widths = state.widths.clone();
    let salt = egui::Id::new(("sqlite_rows", database, object, state.generation));

    ui.scope_builder(
        egui::UiBuilder::new().max_rect(inset(grid_rect)),
        |ui| {
            ui.push_id(salt, |ui| {
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .vscroll(true);
                for (index, _) in page.columns.iter().enumerate() {
                    let width = widths.get(index).copied().unwrap_or(120.0);
                    // The last column doesn't need a resize handle - there is
                    // nothing to its right to resize against, and egui_extras
                    // draws one anyway unless told not to.
                    let last = index + 1 == page.columns.len();
                    table = table.column(if last {
                        // Takes whatever width is left over, so a narrow table
                        // doesn't leave a band of dead space down the right of
                        // the pane - but never narrower than it was fitted to,
                        // so a wide table still scrolls instead of squashing
                        // its last column.
                        Column::remainder().at_least(width).resizable(false)
                    } else {
                        Column::initial(width).at_least(MIN_COLUMN).resizable(true)
                    });
                }

                table
                    .header(ROW_HEIGHT + HEADER_PADDING * 2.0, |mut header| {
                        for name in &page.columns {
                            header.col(|ui| {
                                ui.add_space(HEADER_PADDING);
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(name)
                                            .size(palette.text_size)
                                            .color(palette.text_header_section),
                                    )
                                    .selectable(false),
                                )
                                .on_hover_text(name);
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, page.rows.len(), |mut row| {
                            let cells = &page.rows[row.index()];
                            for cell in cells {
                                row.col(|ui| {
                                    let text = cell.display();
                                    let mut rich =
                                        egui::RichText::new(&text).size(palette.text_size);
                                    // A NULL is a different thing from an
                                    // empty string, so it is dimmed rather
                                    // than drawn as ordinary content.
                                    if cell.is_null() {
                                        rich = rich.weak().italics();
                                    }
                                    let response =
                                        ui.add(egui::Label::new(rich).truncate().selectable(false));
                                    // Only where the tooltip could reveal
                                    // something: a short value is already
                                    // fully on screen, and a tooltip for it is
                                    // just noise following the pointer around.
                                    if needs_tooltip(cell, &text) {
                                        response.on_hover_text(text);
                                    }
                                });
                            }
                        });
                    });
            });
        },
    );

    draw_footer(ui, i18n, palette, state, footer_rect);
}

fn draw_footer(
    ui: &mut egui::Ui,
    i18n: &I18n,
    palette: &ThemePalette,
    state: &mut SqliteState,
    rect: egui::Rect,
) {
    let shown = state.page.rows.len();
    let first = if shown == 0 { 0 } else { state.offset + 1 };
    let last = state.offset + shown;
    let has_prev = state.offset > 0;
    // Only offer the next page when this one came back full - a short page is
    // the end of the object, whatever the (bounded) count says.
    let has_next = shown == sqlite::PAGE_ROWS;

    ui.scope_builder(egui::UiBuilder::new().max_rect(inset(rect)), |ui| {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            let total = match state.total {
                Some(count) if count.capped => format!("{}+", count.value),
                Some(count) => count.value.to_string(),
                None => "?".to_string(),
            };
            // Built from parts rather than one translated sentence: this
            // app's `I18n::tr` takes a key and nothing else, so a message with
            // placeholders has nowhere to put them.
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!(
                        "{} {first}-{last} {} {total}",
                        i18n.tr("sqlite_rows"),
                        i18n.tr("status_of"),
                    ))
                    .size(palette.text_size)
                    .color(palette.text_header_section),
                )
                .selectable(false),
            );

            ui.add_space(12.0);
            if ui
                .add_enabled(
                    has_prev,
                    egui::Button::new(
                        egui::RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::CARET_LEFT,
                            i18n.tr("sqlite_previous")
                        ))
                        .size(palette.text_size),
                    ),
                )
                .clicked()
            {
                state.offset = state.offset.saturating_sub(sqlite::PAGE_ROWS);
            }
            if ui
                .add_enabled(
                    has_next,
                    egui::Button::new(
                        egui::RichText::new(format!(
                            "{} {}",
                            i18n.tr("sqlite_next"),
                            egui_phosphor::regular::CARET_RIGHT
                        ))
                        .size(palette.text_size),
                    ),
                )
                .clicked()
            {
                state.offset += sqlite::PAGE_ROWS;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::LOCK_SIMPLE,
                            i18n.tr("sqlite_read_only")
                        ))
                        .size(palette.text_size)
                        .color(palette.text_header_section),
                    )
                    .selectable(false),
                );
            });
        });
    });
}

/// Whether a cell's full value is worth offering as a tooltip.
fn needs_tooltip(cell: &Cell, text: &str) -> bool {
    /// Roughly what fits in a column at its widest before `truncate` starts
    /// cutting - below this a value is on screen already.
    const LONG: usize = 32;
    matches!(cell, Cell::Text { truncated: true, .. }) || text.chars().count() > LONG
}

/// Senses a double-click on a view's background, for "go up a level".
///
/// Registered *before* the table that sits on top of it, because egui gives
/// an overlapping click to whichever sense was registered later - so a row
/// that is itself clickable (an object in the table list) still wins its own
/// pixels, and only genuinely empty space reaches this. Cells in a row grid
/// are deliberately not clickable, so there the whole grid is background,
/// which is what makes the gesture reachable at all when 500 rows fill the
/// pane and leave no empty space to aim at.
fn background_double_click(ui: &mut egui::Ui, rect: egui::Rect, salt: &str) -> bool {
    ui.interact(rect, ui.id().with(salt), egui::Sense::click())
        .double_clicked()
}

fn inset(rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_max(
        egui::pos2(rect.left() + LEFT_MARGIN, rect.top()),
        rect.right_bottom(),
    )
}

/// Widths that fit the header and the first `MEASURED_ROWS` rows of content.
fn fit_columns(ui: &egui::Ui, page: &Page, text_size: f32) -> Vec<f32> {
    let font = egui::FontId::proportional(text_size);
    let painter = ui.painter();
    let measure = |text: &str| -> f32 {
        painter
            .layout_no_wrap(text.to_string(), font.clone(), egui::Color32::WHITE)
            .rect
            .width()
    };

    page.columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let mut width = measure(name);
            for row in page.rows.iter().take(MEASURED_ROWS) {
                if let Some(cell) = row.get(index) {
                    width = width.max(measure(&display_for_measuring(cell)));
                }
            }
            // Padding for the cell's own margins plus the sort/resize gap.
            (width + 18.0).clamp(MIN_COLUMN, MAX_COLUMN)
        })
        .collect()
}

/// Measuring the *whole* of a 4000-character cell would lay out 4000
/// characters per row per column just to pick a width that is capped at
/// `MAX_COLUMN` anyway - so only the part that could matter is measured.
fn display_for_measuring(cell: &Cell) -> String {
    const ENOUGH: usize = 64;
    let text = cell.display();
    if text.chars().count() > ENOUGH {
        text.chars().take(ENOUGH).collect()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(columns: &[&str], rows: &[&[Cell]]) -> Page {
        Page {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows: rows.iter().map(|r| r.to_vec()).collect(),
        }
    }

    #[test]
    fn a_row_count_says_when_it_was_capped() {
        assert_eq!(
            row_count_text(Some(RowCount {
                value: 12,
                capped: false
            })),
            "12"
        );
        assert_eq!(
            row_count_text(Some(RowCount {
                value: 1_000_000,
                capped: true
            })),
            "1000000+"
        );
        assert_eq!(row_count_text(None), "—");
    }

    #[test]
    fn measuring_a_huge_cell_only_looks_at_its_first_characters() {
        let long: String = std::iter::repeat_n('x', 10_000).collect();
        let cell = Cell::Text {
            text: long,
            truncated: true,
        };
        assert_eq!(display_for_measuring(&cell).chars().count(), 64);
    }

    #[test]
    fn releasing_a_tab_drops_everything_it_was_holding() {
        let mut state = SqliteState {
            showing: Some(Request {
                database: PathBuf::from("a.db"),
                object: Some("t".into()),
                offset: 500,
            }),
            page: page(&["v"], &[&[Cell::Integer(1)]]),
            offset: 500,
            total: Some(RowCount {
                value: 9,
                capped: false,
            }),
            ..Default::default()
        };

        release(&mut state);
        assert!(state.showing.is_none());
        assert!(state.page.rows.is_empty());
        assert_eq!(state.offset, 0);
        assert!(state.total.is_none());
    }


    #[test]
    fn only_values_that_could_be_cut_off_offer_a_tooltip() {
        let short = Cell::Integer(42);
        assert!(!needs_tooltip(&short, &short.display()));

        let long = Cell::Text {
            text: "x".repeat(80),
            truncated: false,
        };
        assert!(needs_tooltip(&long, &long.display()));

        // Truncated by the reader rather than by the column: the full value
        // isn't even in memory, but saying so is still worth a tooltip.
        let cut = Cell::Text {
            text: "short".into(),
            truncated: true,
        };
        assert!(needs_tooltip(&cut, &cut.display()));
    }

    #[test]
    fn a_null_and_an_empty_string_do_not_render_the_same() {
        // They are different values, and a grid that drew both as blank would
        // be lying about the data.
        assert_eq!(Cell::Null.display(), "NULL");
        assert_eq!(
            Cell::Text {
                text: String::new(),
                truncated: false
            }
            .display(),
            ""
        );
    }
}

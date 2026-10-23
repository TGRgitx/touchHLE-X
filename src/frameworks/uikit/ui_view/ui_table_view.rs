/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableView`.
//!
//! A scroll view that shows rows supplied by a data source. Only the rows
//! that intersect the visible area have cells; cells that scroll out of view
//! go into a reuse queue and are handed out again by
//! `dequeueReusableCellWithIdentifier:`.
//!
//! Supported: plain and grouped styles (grouped is only approximated, with
//! inset cells and no rounded corners), variable row heights, section header
//! and footer titles/views, table header and footer views, separators,
//! selection (with the delegate callbacks), scrolling to rows, and the
//! common queries. Changes to the data (including the `insert…`, `delete…`
//! and `reload…` methods) are handled by reloading everything, without
//! animation.
//!
//! TODO: editing mode (delete/insert/move controls), section index, row
//! animations, accessory button callbacks, drawing grouped cells with
//! rounded corners.
//!
//! The table is laid out by [update_pending_tables], which the run loop calls
//! every iteration, since touchHLE doesn't have a general mechanism for
//! `setNeedsLayout`.

use super::ui_scroll_view::UIScrollViewHostObject;
use super::ui_table_view_cell;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::{ns_array, NSInteger};
use crate::frameworks::uikit::ui_color;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, ClassExports, NSZonePtr, SEL,
};
use crate::Environment;

type UITableViewStyle = NSInteger;
const UITableViewStyleGrouped: UITableViewStyle = 1;

type UITableViewCellSeparatorStyle = NSInteger;
const UITableViewCellSeparatorStyleNone: UITableViewCellSeparatorStyle = 0;

type UITableViewScrollPosition = NSInteger;
const UITableViewScrollPositionTop: UITableViewScrollPosition = 1;
const UITableViewScrollPositionMiddle: UITableViewScrollPosition = 2;
const UITableViewScrollPositionBottom: UITableViewScrollPosition = 3;

const DEFAULT_ROW_HEIGHT: CGFloat = 44.0;
const DEFAULT_SEPARATOR_RGBA: (CGFloat, CGFloat, CGFloat, CGFloat) = (0.67, 0.67, 0.67, 1.0);
/// Space around groups of cells in the grouped style.
const GROUPED_INSET: CGFloat = 10.0;

#[derive(Default)]
pub struct State {
    /// All live table views. Non-retaining: tables remove themselves when
    /// deallocated.
    tables: Vec<id>,
}

#[derive(Clone, Default)]
struct SectionInfo {
    row_heights: Vec<CGFloat>,
    header_height: CGFloat,
    footer_height: CGFloat,
    /// `UIView*`, owned by the table, or nil
    header_view: id,
    /// `UIView*`, owned by the table, or nil
    footer_view: id,
    /// The view was made by the table from a title (and so contains a label)
    header_is_title: bool,
    footer_is_title: bool,
}

#[derive(Clone, Copy)]
struct VisibleCell {
    section: usize,
    row: usize,
    /// `UITableViewCell*`, retained by the table
    cell: id,
}

struct UITableViewHostObject {
    superclass: UIScrollViewHostObject,
    style: UITableViewStyle,
    /// Weak reference
    data_source: id,
    row_height: CGFloat,
    /// Zero means "use the default"
    section_header_height: CGFloat,
    section_footer_height: CGFloat,
    separator_style: UITableViewCellSeparatorStyle,
    separator_rgba: (CGFloat, CGFloat, CGFloat, CGFloat),
    allows_selection: bool,
    editing: bool,
    /// `UIView*`, retained
    table_header_view: id,
    /// `UIView*`, retained
    table_footer_view: id,
    sections: Vec<SectionInfo>,
    /// Cells currently in the view hierarchy.
    visible_cells: Vec<VisibleCell>,
    /// `UITableViewCell*`s that are not in use, retained
    reuse_queue: Vec<id>,
    selected: Option<(usize, usize)>,
    needs_reload: bool,
    needs_layout: bool,
    /// Set if everything has to be repositioned, not just newly visible cells
    geometry_dirty: bool,
    in_layout: bool,
    last_width: CGFloat,
}
impl_HostObject_with_superclass!(UITableViewHostObject);
impl Default for UITableViewHostObject {
    fn default() -> Self {
        UITableViewHostObject {
            superclass: Default::default(),
            style: 0,
            data_source: nil,
            row_height: DEFAULT_ROW_HEIGHT,
            section_header_height: 0.0,
            section_footer_height: 0.0,
            separator_style: 1, // UITableViewCellSeparatorStyleSingleLine
            separator_rgba: DEFAULT_SEPARATOR_RGBA,
            allows_selection: true,
            editing: false,
            table_header_view: nil,
            table_footer_view: nil,
            sections: Vec::new(),
            visible_cells: Vec::new(),
            reuse_queue: Vec::new(),
            selected: None,
            needs_reload: true,
            needs_layout: true,
            geometry_dirty: true,
            in_layout: false,
            last_width: -1.0,
        }
    }
}

fn rect(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize { width, height },
    }
}

fn responds(env: &mut Environment, object: id, selector: &str) -> bool {
    if object == nil {
        return false;
    }
    let sel: SEL = env
        .objc
        .register_host_selector(selector.to_string(), &mut env.mem);
    msg![env; object respondsToSelector:sel]
}

fn index_path(env: &mut Environment, section: usize, row: usize) -> id {
    msg_class![env; NSIndexPath indexPathForRow:(row as NSInteger) inSection:(section as NSInteger)]
}

fn separator_for(host: &UITableViewHostObject) -> Option<(CGFloat, CGFloat, CGFloat, CGFloat)> {
    if host.separator_style == UITableViewCellSeparatorStyleNone {
        None
    } else {
        Some(host.separator_rgba)
    }
}

/// Make the view for a section header/footer title. The caller owns it.
fn make_title_view(env: &mut Environment, title: id, style: UITableViewStyle, header: bool) -> id {
    let grouped = style == UITableViewStyleGrouped;
    let container: id = msg_class![env; UIView alloc];
    let container: id = msg![env; container initWithFrame:(rect(0.0, 0.0, 0.0, 0.0))];
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:(rect(0.0, 0.0, 0.0, 0.0))];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; label setBackgroundColor:clear];
    () = msg![env; label setText:title];
    if grouped {
        let font: id = if header {
            msg_class![env; UIFont boldSystemFontOfSize:17.0f32]
        } else {
            msg_class![env; UIFont systemFontOfSize:15.0f32]
        };
        let color: id =
            msg_class![env; UIColor colorWithRed:0.3f32 green:0.34f32 blue:0.42f32 alpha:1.0f32];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextColor:color];
        () = msg![env; container setBackgroundColor:clear];
        if !header {
            () = msg![env; label setTextAlignment:1i32]; // UITextAlignmentCenter
        }
    } else {
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:16.0f32];
        let white: id = msg_class![env; UIColor whiteColor];
        let background: id =
            msg_class![env; UIColor colorWithRed:0.52f32 green:0.58f32 blue:0.64f32 alpha:1.0f32];
        () = msg![env; label setFont:font];
        () = msg![env; label setTextColor:white];
        () = msg![env; container setBackgroundColor:background];
    }
    () = msg![env; container addSubview:label];
    release(env, label);
    container
}

/// Create the header or footer of a section from the delegate and data
/// source. Returns (view or nil, view was made from a title, height). If a
/// view is returned, the caller owns it.
fn make_section_decoration(
    env: &mut Environment,
    this: id,
    data_source: id,
    delegate: id,
    section: usize,
    header: bool,
) -> (id, bool, CGFloat) {
    let (height_sel, view_sel, title_sel) = if header {
        (
            "tableView:heightForHeaderInSection:",
            "tableView:viewForHeaderInSection:",
            "tableView:titleForHeaderInSection:",
        )
    } else {
        (
            "tableView:heightForFooterInSection:",
            "tableView:viewForFooterInSection:",
            "tableView:titleForFooterInSection:",
        )
    };
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let style = host.style;
    let configured_height = if header {
        host.section_header_height
    } else {
        host.section_footer_height
    };
    let section_arg = section as NSInteger;

    let mut view = nil;
    let mut is_title = false;
    if responds(env, delegate, view_sel) {
        let provided: id = if header {
            msg![env; delegate tableView:this viewForHeaderInSection:section_arg]
        } else {
            msg![env; delegate tableView:this viewForFooterInSection:section_arg]
        };
        if provided != nil {
            retain(env, provided);
            view = provided;
        }
    }
    if view == nil && responds(env, data_source, title_sel) {
        let title: id = if header {
            msg![env; data_source tableView:this titleForHeaderInSection:section_arg]
        } else {
            msg![env; data_source tableView:this titleForFooterInSection:section_arg]
        };
        if title != nil {
            view = make_title_view(env, title, style, header);
            is_title = true;
        }
    }

    let default_height = if configured_height > 0.0 {
        configured_height
    } else if style == UITableViewStyleGrouped {
        if view != nil {
            30.0
        } else {
            GROUPED_INSET
        }
    } else if view != nil {
        22.0
    } else {
        0.0
    };
    let height = if responds(env, delegate, height_sel) {
        if header {
            msg![env; delegate tableView:this heightForHeaderInSection:section_arg]
        } else {
            msg![env; delegate tableView:this heightForFooterInSection:section_arg]
        }
    } else if view != nil && !is_title {
        let frame: CGRect = msg![env; view frame];
        if frame.size.height > 0.0 {
            frame.size.height
        } else {
            default_height
        }
    } else {
        default_height
    };
    (view, is_title, height)
}

/// Ask the data source about the contents of the table and rebuild the
/// section information. All cells are moved to the reuse queue.
fn reload_data(env: &mut Environment, this: id) {
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    let visible = std::mem::take(&mut host.visible_cells);
    let old_sections = std::mem::take(&mut host.sections);
    host.selected = None;
    host.needs_reload = false;
    host.needs_layout = true;
    host.geometry_dirty = true;
    let data_source = host.data_source;
    let row_height = host.row_height;

    for visible_cell in visible {
        let cell = visible_cell.cell;
        () = msg![env; cell removeFromSuperview];
        // Ownership of the retain passes to the queue.
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .reuse_queue
            .push(cell);
    }
    for section in old_sections {
        for view in [section.header_view, section.footer_view] {
            if view != nil {
                () = msg![env; view removeFromSuperview];
                release(env, view);
            }
        }
    }

    let mut sections = Vec::new();
    if data_source != nil {
        let delegate: id = msg![env; this delegate];
        let section_count = if responds(env, data_source, "numberOfSectionsInTableView:") {
            let count: NSInteger = msg![env; data_source numberOfSectionsInTableView:this];
            count.max(0) as usize
        } else {
            1
        };
        let has_row_heights = responds(env, delegate, "tableView:heightForRowAtIndexPath:");
        for section in 0..section_count {
            let rows: NSInteger =
                msg![env; data_source tableView:this numberOfRowsInSection:(section as NSInteger)];
            let rows = rows.max(0) as usize;
            let mut row_heights = Vec::with_capacity(rows);
            for row in 0..rows {
                if has_row_heights {
                    let path = index_path(env, section, row);
                    let height: CGFloat =
                        msg![env; delegate tableView:this heightForRowAtIndexPath:path];
                    row_heights.push(height);
                } else {
                    row_heights.push(row_height);
                }
            }
            let (header_view, header_is_title, header_height) =
                make_section_decoration(env, this, data_source, delegate, section, true);
            let (footer_view, footer_is_title, footer_height) =
                make_section_decoration(env, this, data_source, delegate, section, false);
            for view in [header_view, footer_view] {
                if view != nil {
                    () = msg![env; this addSubview:view];
                }
            }
            sections.push(SectionInfo {
                row_heights,
                header_height,
                footer_height,
                header_view,
                footer_view,
                header_is_title,
                footer_is_title,
            });
        }
    }
    env.objc.borrow_mut::<UITableViewHostObject>(this).sections = sections;
}

/// Positions of the parts of the table, in the scroll view's content
/// coordinates.
struct Geometry {
    /// (header top, first row top, footer top) for each section
    section_tops: Vec<(CGFloat, CGFloat, CGFloat)>,
    table_header_height: CGFloat,
    table_footer_top: CGFloat,
    table_footer_height: CGFloat,
    content_height: CGFloat,
}

fn view_height(env: &mut Environment, view: id) -> CGFloat {
    if view == nil {
        return 0.0;
    }
    let frame: CGRect = msg![env; view frame];
    frame.size.height
}

fn geometry(env: &mut Environment, this: id) -> Geometry {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let header = host.table_header_view;
    let footer = host.table_footer_view;
    let sections = host.sections.clone();
    let table_header_height = view_height(env, header);
    let table_footer_height = view_height(env, footer);

    let mut y = table_header_height;
    let mut section_tops = Vec::with_capacity(sections.len());
    for section in &sections {
        let header_top = y;
        y += section.header_height;
        let rows_top = y;
        y += section.row_heights.iter().sum::<CGFloat>();
        let footer_top = y;
        y += section.footer_height;
        section_tops.push((header_top, rows_top, footer_top));
    }
    let table_footer_top = y;
    y += table_footer_height;
    Geometry {
        section_tops,
        table_header_height,
        table_footer_top,
        table_footer_height,
        content_height: y,
    }
}

/// Move a cell no longer needed to the reuse queue.
fn recycle_cell(env: &mut Environment, this: id, cell: id) {
    () = msg![env; cell removeFromSuperview];
    env.objc
        .borrow_mut::<UITableViewHostObject>(this)
        .reuse_queue
        .push(cell);
}

/// Position the parts of the table and make sure exactly the cells that are
/// visible exist. This may call into the data source and delegate.
fn do_layout(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let grouped = host.style == UITableViewStyleGrouped;
    let table_header = host.table_header_view;
    let table_footer = host.table_footer_view;
    let data_source = host.data_source;
    let separator = separator_for(host);
    let sections = host.sections.clone();
    let inset = if grouped { GROUPED_INSET } else { 0.0 };

    let mut bounds: CGRect = msg![env; this bounds];
    let width = bounds.size.width;
    let dirty = {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        host.geometry_dirty || host.last_width != width
    };

    let geometry = geometry(env, this);
    let content = CGSize {
        width,
        height: geometry.content_height,
    };
    () = msg![env; this setContentSize:content];

    // The content may have become shorter than the current scroll position.
    let offset: CGPoint = msg![env; this contentOffset];
    let max_offset_y = (geometry.content_height - bounds.size.height).max(0.0);
    if offset.y > max_offset_y {
        let clamped = CGPoint {
            x: offset.x,
            y: max_offset_y,
        };
        () = msg![env; this setContentOffset:clamped];
        bounds = msg![env; this bounds];
    }
    let view_top = bounds.origin.y;
    let view_bottom = view_top + bounds.size.height;

    // Table header and footer, and section headers and footers
    if dirty {
        if table_header != nil {
            let frame = rect(0.0, 0.0, width, geometry.table_header_height);
            () = msg![env; table_header setFrame:frame];
        }
        if table_footer != nil {
            let frame = rect(
                0.0,
                geometry.table_footer_top,
                width,
                geometry.table_footer_height,
            );
            () = msg![env; table_footer setFrame:frame];
        }
        for (section, &(header_top, _, footer_top)) in sections.iter().zip(&geometry.section_tops) {
            let parts = [
                (
                    section.header_view,
                    section.header_is_title,
                    header_top,
                    section.header_height,
                ),
                (
                    section.footer_view,
                    section.footer_is_title,
                    footer_top,
                    section.footer_height,
                ),
            ];
            for (view, is_title, top, height) in parts {
                if view == nil {
                    continue;
                }
                () = msg![env; view setFrame:(rect(0.0, top, width, height))];
                if is_title {
                    let subviews: id = msg![env; view subviews];
                    let label: id = msg![env; subviews objectAtIndex:0u32];
                    let label_inset = if grouped { 20.0 } else { 10.0 };
                    let label_frame = rect(
                        label_inset,
                        0.0,
                        (width - 2.0 * label_inset).max(0.0),
                        height,
                    );
                    () = msg![env; label setFrame:label_frame];
                    () = msg![env; label setNeedsDisplay];
                }
            }
        }
    }

    // Which rows are visible?
    let mut needed: Vec<(usize, usize, CGFloat, CGFloat)> = Vec::new();
    for (section_idx, (section, &(_, rows_top, _))) in
        sections.iter().zip(&geometry.section_tops).enumerate()
    {
        let mut top = rows_top;
        for (row_idx, &height) in section.row_heights.iter().enumerate() {
            if top + height > view_top && top < view_bottom {
                needed.push((section_idx, row_idx, top, height));
            }
            top += height;
        }
    }

    // Recycle cells that are no longer visible.
    let current = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .clone();
    let mut kept = Vec::new();
    for visible_cell in current {
        let still_needed = needed
            .iter()
            .any(|&(s, r, ..)| s == visible_cell.section && r == visible_cell.row);
        if still_needed {
            kept.push(visible_cell);
        } else {
            recycle_cell(env, this, visible_cell.cell);
        }
    }
    env.objc
        .borrow_mut::<UITableViewHostObject>(this)
        .visible_cells = kept.clone();

    // Reposition existing cells if the table changed width
    if dirty {
        for visible_cell in &kept {
            let (_, _, top, height) = needed
                .iter()
                .copied()
                .find(|&(s, r, ..)| s == visible_cell.section && r == visible_cell.row)
                .unwrap();
            let cell = visible_cell.cell;
            let frame = rect(inset, top, (width - 2.0 * inset).max(0.0), height);
            () = msg![env; cell setFrame:frame];
            () = msg![env; cell layoutSubviews];
        }
    }

    // Make cells for rows that just became visible.
    let delegate: id = msg![env; this delegate];
    for (section, row, top, height) in needed {
        if kept.iter().any(|c| c.section == section && c.row == row) {
            continue;
        }
        let path = index_path(env, section, row);
        let cell: id = msg![env; data_source tableView:this cellForRowAtIndexPath:path];
        if cell == nil {
            log!(
                "Warning: data source {:?} returned nil from tableView:cellForRowAtIndexPath: for section {} row {}",
                data_source, section, row
            );
            continue;
        }
        retain(env, cell);
        let frame = rect(inset, top, (width - 2.0 * inset).max(0.0), height);
        () = msg![env; cell setFrame:frame];
        () = msg![env; this addSubview:cell];
        () = msg![env; cell layoutSubviews];
        ui_table_view_cell::set_separator(env, cell, separator);
        if env.objc.borrow::<UITableViewHostObject>(this).selected == Some((section, row)) {
            () = msg![env; cell setSelected:true animated:false];
        }
        if responds(
            env,
            delegate,
            "tableView:willDisplayCell:forRowAtIndexPath:",
        ) {
            () = msg![env; delegate tableView:this willDisplayCell:cell forRowAtIndexPath:path];
        }
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .visible_cells
            .push(VisibleCell { section, row, cell });
    }

    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    host.last_width = width;
    host.geometry_dirty = false;
}

/// Bring the table up to date: reload if needed, then lay out.
fn layout(env: &mut Environment, this: id) {
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    if host.in_layout || host.data_source == nil {
        return;
    }
    host.in_layout = true;
    let needs_reload = host.needs_reload;

    let pool: id = msg_class![env; NSAutoreleasePool new];
    if needs_reload {
        reload_data(env, this);
    }
    do_layout(env, this);
    let _: () = msg![env; pool drain];

    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    host.in_layout = false;
    host.needs_layout = false;
}

/// Bring the table up to date if there are pending changes.
fn refresh(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    if (host.needs_layout || host.needs_reload) && host.data_source != nil {
        layout(env, this);
    }
}

/// Called by the run loop every iteration. Lays out tables that need it.
pub fn update_pending_tables(env: &mut Environment) {
    let tables = env
        .framework_state
        .uikit
        .ui_view
        .ui_table_view
        .tables
        .clone();
    for table in tables {
        let host = env.objc.borrow::<UITableViewHostObject>(table);
        if !(host.needs_layout || host.needs_reload) || host.data_source == nil {
            continue;
        }
        // Like real UIKit, don't bother with tables that aren't on screen.
        let window: id = msg![env; table window];
        if window == nil {
            continue;
        }
        retain(env, table);
        layout(env, table);
        release(env, table);
    }
}

fn mark_for_reload(env: &mut Environment, this: id) {
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    host.needs_reload = true;
    host.needs_layout = true;
}

fn mark_for_layout(env: &mut Environment, this: id) {
    env.objc
        .borrow_mut::<UITableViewHostObject>(this)
        .needs_layout = true;
}

fn find_visible_cell(env: &Environment, this: id, section: usize, row: usize) -> Option<id> {
    env.objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .iter()
        .find(|c| c.section == section && c.row == row)
        .map(|c| c.cell)
}

fn set_row_selected(env: &mut Environment, this: id, section: usize, row: usize, selected: bool) {
    if let Some(cell) = find_visible_cell(env, this, section, row) {
        () = msg![env; cell setSelected:selected animated:false];
    }
}

fn select_row(env: &mut Environment, this: id, section: usize, row: usize) {
    let previous = env.objc.borrow::<UITableViewHostObject>(this).selected;
    if let Some((s, r)) = previous {
        set_row_selected(env, this, s, r, false);
    }
    env.objc.borrow_mut::<UITableViewHostObject>(this).selected = Some((section, row));
    set_row_selected(env, this, section, row, true);
}

fn deselect_row(env: &mut Environment, this: id) {
    let previous = env
        .objc
        .borrow_mut::<UITableViewHostObject>(this)
        .selected
        .take();
    if let Some((s, r)) = previous {
        set_row_selected(env, this, s, r, false);
    }
}

/// Frame of a row in the content coordinates, if it exists.
fn row_rect(env: &mut Environment, this: id, section: usize, row: usize) -> Option<CGRect> {
    refresh(env, this);
    let geometry = geometry(env, this);
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let inset = if host.style == UITableViewStyleGrouped {
        GROUPED_INSET
    } else {
        0.0
    };
    let info = host.sections.get(section)?;
    let height = *info.row_heights.get(row)?;
    let top = geometry.section_tops[section].1 + info.row_heights[..row].iter().sum::<CGFloat>();
    let bounds: CGRect = msg![env; this bounds];
    Some(rect(
        inset,
        top,
        (bounds.size.width - 2.0 * inset).max(0.0),
        height,
    ))
}

fn path_to_indices(env: &mut Environment, path: id) -> Option<(usize, usize)> {
    if path == nil {
        return None;
    }
    let section: NSInteger = msg![env; path section];
    let row: NSInteger = msg![env; path row];
    if section < 0 || row < 0 {
        return None;
    }
    Some((section as usize, row as usize))
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableView: UIScrollView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewHostObject>::default();
    let new = env.objc.alloc_object(this, host_object, &mut env.mem);
    env.framework_state.uikit.ui_view.ui_table_view.tables.push(new);
    new
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; this setBackgroundColor:white];
    () = msg![env; this setClipsToBounds:true];
    this
}

- (id)initWithFrame:(CGRect)frame
              style:(UITableViewStyle)style {
    let this: id = msg![env; this initWithFrame:frame];
    env.objc.borrow_mut::<UITableViewHostObject>(this).style = style;
    if style == UITableViewStyleGrouped {
        let background: id = msg_class![env; UIColor colorWithRed:0.78f32 green:0.8f32 blue:0.84f32 alpha:1.0f32];
        () = msg![env; this setBackgroundColor:background];
    }
    this
}

- (())dealloc {
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    let visible = std::mem::take(&mut host.visible_cells);
    let queue = std::mem::take(&mut host.reuse_queue);
    let sections = std::mem::take(&mut host.sections);
    let table_header = host.table_header_view;
    let table_footer = host.table_footer_view;

    // Cells and views in the hierarchy are also owned by this view as
    // subviews, which UIView's dealloc will release.
    for visible_cell in visible {
        release(env, visible_cell.cell);
    }
    for cell in queue {
        release(env, cell);
    }
    for section in sections {
        release(env, section.header_view);
        release(env, section.footer_view);
    }
    release(env, table_header);
    release(env, table_footer);

    let tables = &mut env.framework_state.uikit.ui_view.ui_table_view.tables;
    tables.retain(|&table| table != this);

    msg_super![env; this dealloc]
}

- (UITableViewStyle)style {
    env.objc.borrow::<UITableViewHostObject>(this).style
}

// Weak reference
- (id)dataSource {
    env.objc.borrow::<UITableViewHostObject>(this).data_source
}
- (())setDataSource:(id)data_source {
    env.objc.borrow_mut::<UITableViewHostObject>(this).data_source = data_source;
    mark_for_reload(env, this);
}

- (())setContentOffset:(CGPoint)offset {
    msg_super![env; this setContentOffset:offset];
    mark_for_layout(env, this);
}

- (())setFrame:(CGRect)frame {
    msg_super![env; this setFrame:frame];
    mark_for_layout(env, this);
}

// Configuration

- (CGFloat)rowHeight {
    env.objc.borrow::<UITableViewHostObject>(this).row_height
}
- (())setRowHeight:(CGFloat)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).row_height = height;
    mark_for_reload(env, this);
}

- (CGFloat)sectionHeaderHeight {
    env.objc.borrow::<UITableViewHostObject>(this).section_header_height
}
- (())setSectionHeaderHeight:(CGFloat)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_header_height = height;
    mark_for_reload(env, this);
}

- (CGFloat)sectionFooterHeight {
    env.objc.borrow::<UITableViewHostObject>(this).section_footer_height
}
- (())setSectionFooterHeight:(CGFloat)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_footer_height = height;
    mark_for_reload(env, this);
}

- (UITableViewCellSeparatorStyle)separatorStyle {
    env.objc.borrow::<UITableViewHostObject>(this).separator_style
}
- (())setSeparatorStyle:(UITableViewCellSeparatorStyle)style {
    env.objc.borrow_mut::<UITableViewHostObject>(this).separator_style = style;
    apply_separators(env, this);
}

- (())setSeparatorColor:(id)color { // UIColor*
    let rgba = if color == nil {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        ui_color::get_rgba(&env.objc, color)
    };
    env.objc.borrow_mut::<UITableViewHostObject>(this).separator_rgba = rgba;
    apply_separators(env, this);
}

- (bool)allowsSelection {
    env.objc.borrow::<UITableViewHostObject>(this).allows_selection
}
- (())setAllowsSelection:(bool)allows {
    env.objc.borrow_mut::<UITableViewHostObject>(this).allows_selection = allows;
}

- (bool)isEditing {
    env.objc.borrow::<UITableViewHostObject>(this).editing
}
- (())setEditing:(bool)editing {
    // TODO: editing controls
    env.objc.borrow_mut::<UITableViewHostObject>(this).editing = editing;
}
- (())setEditing:(bool)editing animated:(bool)_animated {
    () = msg![env; this setEditing:editing];
}

- (id)tableHeaderView {
    env.objc.borrow::<UITableViewHostObject>(this).table_header_view
}
- (())setTableHeaderView:(id)view { // UIView*
    let old = env.objc.borrow::<UITableViewHostObject>(this).table_header_view;
    set_table_accessory_view(env, this, old, view, true);
}

- (id)tableFooterView {
    env.objc.borrow::<UITableViewHostObject>(this).table_footer_view
}
- (())setTableFooterView:(id)view { // UIView*
    let old = env.objc.borrow::<UITableViewHostObject>(this).table_footer_view;
    set_table_accessory_view(env, this, old, view, false);
}

// Loading data

- (())reloadData {
    mark_for_reload(env, this);
}

// These should animate, but just reload everything instead.
- (())beginUpdates {
}
- (())endUpdates {
}
- (())insertRowsAtIndexPaths:(id)_paths // NSArray*
            withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}
- (())deleteRowsAtIndexPaths:(id)_paths // NSArray*
            withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}
- (())reloadRowsAtIndexPaths:(id)_paths // NSArray*
            withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}
- (())insertSections:(id)_sections // NSIndexSet*
    withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}
- (())deleteSections:(id)_sections // NSIndexSet*
    withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}
- (())reloadSections:(id)_sections // NSIndexSet*
    withRowAnimation:(NSInteger)_animation {
    mark_for_reload(env, this);
}

// Cells

- (id)dequeueReusableCellWithIdentifier:(id)identifier { // NSString*
    if identifier == nil {
        return nil;
    }
    let queue = env.objc.borrow::<UITableViewHostObject>(this).reuse_queue.clone();
    for (index, cell) in queue.into_iter().enumerate() {
        let cell_identifier = ui_table_view_cell::reuse_identifier(env, cell);
        if cell_identifier == nil {
            continue;
        }
        let matches: bool = msg![env; cell_identifier isEqualToString:identifier];
        if matches {
            // The queue's retain is passed on to the autorelease pool
            env.objc
                .borrow_mut::<UITableViewHostObject>(this)
                .reuse_queue
                .remove(index);
            () = msg![env; cell prepareForReuse];
            return autorelease(env, cell);
        }
    }
    nil
}

- (id)cellForRowAtIndexPath:(id)path { // NSIndexPath*
    refresh(env, this);
    let Some((section, row)) = path_to_indices(env, path) else {
        return nil;
    };
    find_visible_cell(env, this, section, row).unwrap_or(nil)
}

- (id)visibleCells {
    refresh(env, this);
    let mut visible = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .clone();
    visible.sort_by_key(|c| (c.section, c.row));
    let cells = visible.iter().map(|c| c.cell).collect();
    let array = ns_array::from_vec(env, cells);
    autorelease(env, array)
}

- (id)indexPathsForVisibleRows {
    refresh(env, this);
    let mut visible = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .clone();
    visible.sort_by_key(|c| (c.section, c.row));
    let mut paths = Vec::with_capacity(visible.len());
    for c in visible {
        paths.push(index_path(env, c.section, c.row));
    }
    let array = ns_array::from_vec(env, paths);
    autorelease(env, array)
}

- (id)indexPathForCell:(id)cell { // UITableViewCell*
    refresh(env, this);
    let found = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .iter()
        .find(|c| c.cell == cell)
        .copied();
    match found {
        Some(c) => index_path(env, c.section, c.row),
        None => nil,
    }
}

- (id)indexPathForRowAtPoint:(CGPoint)point {
    refresh(env, this);
    let geometry = geometry(env, this);
    let sections = env.objc.borrow::<UITableViewHostObject>(this).sections.clone();
    for (section, (info, &(_, rows_top, _))) in sections.iter().zip(&geometry.section_tops).enumerate() {
        let mut top = rows_top;
        for (row, &height) in info.row_heights.iter().enumerate() {
            if point.y >= top && point.y < top + height {
                return index_path(env, section, row);
            }
            top += height;
        }
    }
    nil
}

- (CGRect)rectForRowAtIndexPath:(id)path { // NSIndexPath*
    let Some((section, row)) = path_to_indices(env, path) else {
        return rect(0.0, 0.0, 0.0, 0.0);
    };
    row_rect(env, this, section, row).unwrap_or(rect(0.0, 0.0, 0.0, 0.0))
}

- (CGRect)rectForSection:(NSInteger)section {
    refresh(env, this);
    let geometry = geometry(env, this);
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let (Some(info), Some(&(header_top, _, footer_top))) = (
        host.sections.get(section.max(0) as usize),
        geometry.section_tops.get(section.max(0) as usize),
    ) else {
        return rect(0.0, 0.0, 0.0, 0.0);
    };
    let bottom = footer_top + info.footer_height;
    let bounds: CGRect = msg![env; this bounds];
    rect(0.0, header_top, bounds.size.width, bottom - header_top)
}

- (NSInteger)numberOfSections {
    refresh(env, this);
    env.objc.borrow::<UITableViewHostObject>(this).sections.len() as NSInteger
}

- (NSInteger)numberOfRowsInSection:(NSInteger)section {
    refresh(env, this);
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    host.sections
        .get(section.max(0) as usize)
        .map_or(0, |s| s.row_heights.len()) as NSInteger
}

// Selection

- (id)indexPathForSelectedRow {
    let selected = env.objc.borrow::<UITableViewHostObject>(this).selected;
    match selected {
        Some((section, row)) => index_path(env, section, row),
        None => nil,
    }
}

- (())selectRowAtIndexPath:(id)path // NSIndexPath*
                  animated:(bool)_animated
            scrollPosition:(UITableViewScrollPosition)scroll_position {
    refresh(env, this);
    if path == nil {
        deselect_row(env, this);
        return;
    }
    if let Some((section, row)) = path_to_indices(env, path) {
        select_row(env, this, section, row);
        if scroll_position != 0 {
            () = msg![env; this scrollToRowAtIndexPath:path
                                      atScrollPosition:scroll_position
                                              animated:false];
        }
    }
}

- (())deselectRowAtIndexPath:(id)path // NSIndexPath*
                    animated:(bool)_animated {
    let selected = env.objc.borrow::<UITableViewHostObject>(this).selected;
    if selected.is_some() && selected == path_to_indices(env, path) {
        deselect_row(env, this);
    }
}

// Called by UITableViewCell when it has been tapped
- (())_touchHLE_cellTapped:(id)cell {
    let found = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .iter()
        .find(|c| c.cell == cell)
        .copied();
    let Some(found) = found else {
        return;
    };
    if !env.objc.borrow::<UITableViewHostObject>(this).allows_selection {
        return;
    }

    let delegate: id = msg![env; this delegate];
    let mut path = index_path(env, found.section, found.row);
    if responds(env, delegate, "tableView:willSelectRowAtIndexPath:") {
        path = msg![env; delegate tableView:this willSelectRowAtIndexPath:path];
    }
    let Some((section, row)) = path_to_indices(env, path) else {
        return;
    };
    select_row(env, this, section, row);
    if responds(env, delegate, "tableView:didSelectRowAtIndexPath:") {
        () = msg![env; delegate tableView:this didSelectRowAtIndexPath:path];
    }
}

// Scrolling

- (())scrollToRowAtIndexPath:(id)path // NSIndexPath*
            atScrollPosition:(UITableViewScrollPosition)position
                    animated:(bool)_animated {
    let Some((section, row)) = path_to_indices(env, path) else {
        return;
    };
    let Some(row_rect) = row_rect(env, this, section, row) else {
        return;
    };
    let bounds: CGRect = msg![env; this bounds];
    let content: CGSize = msg![env; this contentSize];
    let current = bounds.origin.y;
    let target = match position {
        UITableViewScrollPositionTop => row_rect.origin.y,
        UITableViewScrollPositionMiddle => {
            row_rect.origin.y + row_rect.size.height / 2.0 - bounds.size.height / 2.0
        }
        UITableViewScrollPositionBottom => {
            row_rect.origin.y + row_rect.size.height - bounds.size.height
        }
        // Scroll the minimum amount to make the row visible
        _ => {
            if row_rect.origin.y < current {
                row_rect.origin.y
            } else if row_rect.origin.y + row_rect.size.height > current + bounds.size.height {
                row_rect.origin.y + row_rect.size.height - bounds.size.height
            } else {
                current
            }
        }
    };
    let max_y = (content.height - bounds.size.height).max(0.0);
    let new_offset = CGPoint {
        x: bounds.origin.x,
        y: target.clamp(0.0, max_y),
    };
    () = msg![env; this setContentOffset:new_offset];
}

@end

};

/// Apply the table's separator settings to all visible cells.
fn apply_separators(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let separator = separator_for(host);
    let cells: Vec<id> = host.visible_cells.iter().map(|c| c.cell).collect();
    for cell in cells {
        ui_table_view_cell::set_separator(env, cell, separator);
    }
}

/// Shared by `setTableHeaderView:` and `setTableFooterView:`.
fn set_table_accessory_view(env: &mut Environment, this: id, old: id, new: id, header: bool) {
    if old == new {
        return;
    }
    retain(env, new);
    if old != nil {
        () = msg![env; old removeFromSuperview];
        release(env, old);
    }
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    if header {
        host.table_header_view = new;
    } else {
        host.table_footer_view = new;
    }
    host.needs_layout = true;
    host.geometry_dirty = true;
    if new != nil {
        () = msg![env; this addSubview:new];
    }
}

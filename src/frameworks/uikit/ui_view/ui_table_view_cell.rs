/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableViewCell`.
//!
//! Supports the four built-in cell styles, a content view, the text, detail
//! text and image views, accessory types (drawn as simple glyphs),
//! accessory views and selection/highlight appearance.
//!
//! TODO: editing controls, indentation, background/selected background views
//! and the detail disclosure button.

use crate::frameworks::core_graphics::cg_context::{CGContextFillRect, CGContextSetRGBFillColor};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, todo_objc_setter, ClassExports, NSZonePtr,
};
use crate::Environment;

pub type UITableViewCellStyle = NSInteger;
const UITableViewCellStyleDefault: UITableViewCellStyle = 0;
const UITableViewCellStyleValue1: UITableViewCellStyle = 1;
const UITableViewCellStyleValue2: UITableViewCellStyle = 2;
const UITableViewCellStyleSubtitle: UITableViewCellStyle = 3;

type UITableViewCellAccessoryType = NSInteger;
const UITableViewCellAccessoryNone: UITableViewCellAccessoryType = 0;
const UITableViewCellAccessoryDisclosureIndicator: UITableViewCellAccessoryType = 1;
const UITableViewCellAccessoryDetailDisclosureButton: UITableViewCellAccessoryType = 2;
const UITableViewCellAccessoryCheckmark: UITableViewCellAccessoryType = 3;

type UITableViewCellSelectionStyle = NSInteger;
const UITableViewCellSelectionStyleNone: UITableViewCellSelectionStyle = 0;
const UITableViewCellSelectionStyleGray: UITableViewCellSelectionStyle = 2;

/// UITextAlignmentRight
const TEXT_ALIGNMENT_RIGHT: NSInteger = 2;

/// Distance (in points) a touch can move before it no longer counts as a tap.
const TAP_SLOP: CGFloat = 10.0;

const SELECTION_BLUE: (CGFloat, CGFloat, CGFloat) = (0.02, 0.43, 0.93);
const SELECTION_GRAY: (CGFloat, CGFloat, CGFloat) = (0.78, 0.78, 0.8);
const LABEL_BLUE: (CGFloat, CGFloat, CGFloat) = (0.22, 0.33, 0.53);

struct UITableViewCellHostObject {
    superclass: super::UIViewHostObject,
    style: UITableViewCellStyle,
    /// `NSString*`, retained
    reuse_identifier: id,
    /// `UIView*`, owned
    content_view: id,
    /// `UILabel*`, owned, created lazily
    text_label: id,
    /// `UILabel*`, owned, created lazily
    detail_text_label: id,
    /// `UIImageView*`, owned, created lazily
    image_view: id,
    /// `UIView*`, owned
    accessory_view: id,
    accessory_type: UITableViewCellAccessoryType,
    selection_style: UITableViewCellSelectionStyle,
    selected: bool,
    highlighted: bool,
    /// Colour of the separator line drawn at the bottom of the cell, set by
    /// the table view.
    separator_rgba: Option<(CGFloat, CGFloat, CGFloat, CGFloat)>,
    /// Where the current touch started, if it could still turn out to be a tap.
    touch_start: Option<CGPoint>,
    /// `UIColor*` of the text label before it was changed to indicate
    /// selection, retained.
    saved_text_color: id,
    /// Same for the detail text label.
    saved_detail_color: id,
    /// `UIColor*` of the cell before it was changed to indicate selection,
    /// retained.
    saved_background: id,
    /// The selection/highlight appearance is currently applied.
    showing_selection: bool,
}
impl_HostObject_with_superclass!(UITableViewCellHostObject);
impl Default for UITableViewCellHostObject {
    fn default() -> Self {
        UITableViewCellHostObject {
            superclass: Default::default(),
            style: UITableViewCellStyleDefault,
            reuse_identifier: nil,
            content_view: nil,
            text_label: nil,
            detail_text_label: nil,
            image_view: nil,
            accessory_view: nil,
            accessory_type: UITableViewCellAccessoryNone,
            selection_style: 1, // UITableViewCellSelectionStyleBlue
            selected: false,
            highlighted: false,
            separator_rgba: None,
            touch_start: None,
            saved_text_color: nil,
            saved_detail_color: nil,
            saved_background: nil,
            showing_selection: false,
        }
    }
}

fn rect(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize { width, height },
    }
}

/// Called by the table view to set the separator line shown below a cell.
pub fn set_separator(
    env: &mut Environment,
    cell: id,
    rgba: Option<(CGFloat, CGFloat, CGFloat, CGFloat)>,
) {
    env.objc
        .borrow_mut::<UITableViewCellHostObject>(cell)
        .separator_rgba = rgba;
    () = msg![env; cell setNeedsDisplay];
}

/// Returns `reuseIdentifier` (an `NSString*` or nil).
pub fn reuse_identifier(env: &Environment, cell: id) -> id {
    env.objc
        .borrow::<UITableViewCellHostObject>(cell)
        .reuse_identifier
}

fn new_label(
    env: &mut Environment,
    cell: id,
    size: CGFloat,
    bold: bool,
    rgb: (CGFloat, CGFloat, CGFloat),
) -> id {
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:(rect(0.0, 0.0, 0.0, 0.0))];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; label setBackgroundColor:clear];
    let font: id = if bold {
        msg_class![env; UIFont boldSystemFontOfSize:size]
    } else {
        msg_class![env; UIFont systemFontOfSize:size]
    };
    () = msg![env; label setFont:font];
    let (r, g, b) = rgb;
    let color: id = msg_class![env; UIColor colorWithRed:r green:g blue:b alpha:1.0f32];
    () = msg![env; label setTextColor:color];
    let content_view = env
        .objc
        .borrow::<UITableViewCellHostObject>(cell)
        .content_view;
    () = msg![env; content_view addSubview:label];
    label
}

fn host_showing(env: &Environment, cell: id) -> bool {
    env.objc
        .borrow::<UITableViewCellHostObject>(cell)
        .showing_selection
}

/// Update background colour and text colours according to the
/// selected/highlighted state.
fn update_appearance(env: &mut Environment, cell: id) {
    let host = env.objc.borrow::<UITableViewCellHostObject>(cell);
    let on = (host.selected || host.highlighted)
        && host.selection_style != UITableViewCellSelectionStyleNone;
    let gray = host.selection_style == UITableViewCellSelectionStyleGray;
    let text_label = host.text_label;
    let detail_label = host.detail_text_label;
    let saved_text = host.saved_text_color;
    let saved_detail = host.saved_detail_color;

    if on {
        if !host_showing(env, cell) {
            let background: id = msg![env; cell backgroundColor];
            retain(env, background);
            let host = env.objc.borrow_mut::<UITableViewCellHostObject>(cell);
            host.saved_background = background;
            host.showing_selection = true;
        }
        let (r, g, b) = if gray { SELECTION_GRAY } else { SELECTION_BLUE };
        let color: id = msg_class![env; UIColor colorWithRed:r green:g blue:b alpha:1.0f32];
        () = msg![env; cell setBackgroundColor:color];
        if !gray {
            let white: id = msg_class![env; UIColor whiteColor];
            // Only remember the original colours when first becoming "on".
            if saved_text == nil && text_label != nil {
                let old: id = msg![env; text_label textColor];
                retain(env, old);
                env.objc
                    .borrow_mut::<UITableViewCellHostObject>(cell)
                    .saved_text_color = old;
                () = msg![env; text_label setTextColor:white];
            }
            if saved_detail == nil && detail_label != nil {
                let old: id = msg![env; detail_label textColor];
                retain(env, old);
                env.objc
                    .borrow_mut::<UITableViewCellHostObject>(cell)
                    .saved_detail_color = old;
                () = msg![env; detail_label setTextColor:white];
            }
        }
    } else {
        if host_showing(env, cell) {
            let host = env.objc.borrow_mut::<UITableViewCellHostObject>(cell);
            let background = std::mem::replace(&mut host.saved_background, nil);
            host.showing_selection = false;
            () = msg![env; cell setBackgroundColor:background];
            release(env, background);
        }
        if saved_text != nil {
            () = msg![env; text_label setTextColor:saved_text];
            release(env, saved_text);
            env.objc
                .borrow_mut::<UITableViewCellHostObject>(cell)
                .saved_text_color = nil;
        }
        if saved_detail != nil {
            () = msg![env; detail_label setTextColor:saved_detail];
            release(env, saved_detail);
            env.objc
                .borrow_mut::<UITableViewCellHostObject>(cell)
                .saved_detail_color = nil;
        }
    }
    () = msg![env; cell setNeedsDisplay];
}

/// Width reserved at the right of the cell for the accessory.
fn accessory_width(env: &mut Environment, cell: id) -> CGFloat {
    let host = env.objc.borrow::<UITableViewCellHostObject>(cell);
    let accessory_view = host.accessory_view;
    let accessory_type = host.accessory_type;
    if accessory_view != nil {
        let frame: CGRect = msg![env; accessory_view frame];
        return frame.size.width + 20.0;
    }
    match accessory_type {
        UITableViewCellAccessoryDisclosureIndicator | UITableViewCellAccessoryCheckmark => 30.0,
        UITableViewCellAccessoryDetailDisclosureButton => 40.0,
        _ => 0.0,
    }
}

fn set_label_frame(env: &mut Environment, label: id, frame: CGRect) {
    if label != nil {
        () = msg![env; label setFrame:frame];
        () = msg![env; label setNeedsDisplay];
    }
}

/// Fill a small square, used to build up glyphs without path drawing support.
fn fill_block(env: &mut Environment, x: CGFloat, y: CGFloat) {
    let context = UIGraphicsGetCurrentContext(env);
    CGContextFillRect(env, context, rect(x, y, 2.0, 2.0));
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableViewCell: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewCellHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithStyle:(UITableViewCellStyle)style
     reuseIdentifier:(id)reuse_identifier { // NSString*
    let frame = rect(0.0, 0.0, 320.0, 44.0);
    let this: id = msg_super![env; this initWithFrame:frame];
    init_cell(env, this, style, reuse_identifier)
}

// iPhone OS 2.0 designated initializer
- (id)initWithFrame:(CGRect)frame
     reuseIdentifier:(id)reuse_identifier { // NSString*
    let this: id = msg_super![env; this initWithFrame:frame];
    init_cell(env, this, UITableViewCellStyleDefault, reuse_identifier)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    init_cell(env, this, UITableViewCellStyleDefault, nil)
}

- (())dealloc {
    let &UITableViewCellHostObject {
        reuse_identifier,
        content_view,
        text_label,
        detail_text_label,
        image_view,
        accessory_view,
        saved_text_color,
        saved_detail_color,
        saved_background,
        ..
    } = env.objc.borrow(this);
    release(env, reuse_identifier);
    release(env, content_view);
    release(env, text_label);
    release(env, detail_text_label);
    release(env, image_view);
    release(env, accessory_view);
    release(env, saved_text_color);
    release(env, saved_detail_color);
    release(env, saved_background);
    msg_super![env; this dealloc]
}

- (id)reuseIdentifier {
    env.objc.borrow::<UITableViewCellHostObject>(this).reuse_identifier
}

- (id)contentView {
    env.objc.borrow::<UITableViewCellHostObject>(this).content_view
}

- (id)textLabel {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if existing != nil {
        return existing;
    }
    let style = env.objc.borrow::<UITableViewCellHostObject>(this).style;
    let label = match style {
        UITableViewCellStyleValue2 => new_label(env, this, 12.0, true, LABEL_BLUE),
        _ => new_label(env, this, 17.0, true, (0.0, 0.0, 0.0)),
    };
    if style == UITableViewCellStyleValue2 {
        () = msg![env; label setTextAlignment:TEXT_ALIGNMENT_RIGHT];
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).text_label = label;
    label
}

- (id)detailTextLabel {
    let host = env.objc.borrow::<UITableViewCellHostObject>(this);
    // Only available in the styles that show detail text.
    if host.style == UITableViewCellStyleDefault {
        return nil;
    }
    let existing = host.detail_text_label;
    let style = host.style;
    if existing != nil {
        return existing;
    }
    let label = match style {
        UITableViewCellStyleValue1 => new_label(env, this, 17.0, false, LABEL_BLUE),
        UITableViewCellStyleValue2 => new_label(env, this, 15.0, true, (0.0, 0.0, 0.0)),
        _ => new_label(env, this, 14.0, false, (0.5, 0.5, 0.5)),
    };
    if style == UITableViewCellStyleValue1 {
        () = msg![env; label setTextAlignment:TEXT_ALIGNMENT_RIGHT];
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).detail_text_label = label;
    label
}

- (id)imageView {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).image_view;
    if existing != nil {
        return existing;
    }
    let image_view: id = msg_class![env; UIImageView alloc];
    let image_view: id = msg![env; image_view initWithFrame:(rect(0.0, 0.0, 0.0, 0.0))];
    let content_view = env.objc.borrow::<UITableViewCellHostObject>(this).content_view;
    () = msg![env; content_view addSubview:image_view];
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).image_view = image_view;
    image_view
}

// iPhone OS 2.0 convenience properties, forwarded to the text label/image
- (id)text {
    let label: id = msg![env; this textLabel];
    msg![env; label text]
}
- (())setText:(id)text { // NSString*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setText:text];
}
- (id)font {
    let label: id = msg![env; this textLabel];
    msg![env; label font]
}
- (())setFont:(id)font { // UIFont*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setFont:font];
}
- (())setTextColor:(id)color { // UIColor*
    let label: id = msg![env; this textLabel];
    () = msg![env; label setTextColor:color];
}
- (id)image {
    let image_view: id = msg![env; this imageView];
    msg![env; image_view image]
}
- (())setImage:(id)image { // UIImage*
    let image_view: id = msg![env; this imageView];
    () = msg![env; image_view setImage:image];
}

- (UITableViewCellAccessoryType)accessoryType {
    env.objc.borrow::<UITableViewCellHostObject>(this).accessory_type
}
- (())setAccessoryType:(UITableViewCellAccessoryType)accessory_type {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).accessory_type = accessory_type;
    () = msg![env; this layoutSubviews];
    () = msg![env; this setNeedsDisplay];
}

- (id)accessoryView {
    env.objc.borrow::<UITableViewCellHostObject>(this).accessory_view
}
- (())setAccessoryView:(id)view { // UIView*
    let old = env.objc.borrow::<UITableViewCellHostObject>(this).accessory_view;
    if old == view {
        return;
    }
    retain(env, view);
    if old != nil {
        () = msg![env; old removeFromSuperview];
        release(env, old);
    }
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).accessory_view = view;
    if view != nil {
        () = msg![env; this addSubview:view];
    }
    () = msg![env; this layoutSubviews];
    () = msg![env; this setNeedsDisplay];
}

- (UITableViewCellSelectionStyle)selectionStyle {
    env.objc.borrow::<UITableViewCellHostObject>(this).selection_style
}
- (())setSelectionStyle:(UITableViewCellSelectionStyle)selection_style {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selection_style = selection_style;
    update_appearance(env, this);
}

- (bool)isSelected {
    env.objc.borrow::<UITableViewCellHostObject>(this).selected
}
- (())setSelected:(bool)selected {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected = selected;
    update_appearance(env, this);
}
- (())setSelected:(bool)selected animated:(bool)_animated {
    // TODO: animate
    () = msg![env; this setSelected:selected];
}

- (bool)isHighlighted {
    env.objc.borrow::<UITableViewCellHostObject>(this).highlighted
}
- (())setHighlighted:(bool)highlighted {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).highlighted = highlighted;
    update_appearance(env, this);
}
- (())setHighlighted:(bool)highlighted animated:(bool)_animated {
    // TODO: animate
    () = msg![env; this setHighlighted:highlighted];
}

- (())prepareForReuse {
    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    host.selected = false;
    host.highlighted = false;
    host.touch_start = None;
    update_appearance(env, this);
}

- (())setIndentationLevel:(NSInteger)level {
    todo_objc_setter!(this, level);
}
- (())setIndentationWidth:(CGFloat)width {
    todo_objc_setter!(this, width);
}
- (())setSelectedBackgroundView:(id)view {
    todo_objc_setter!(this, view);
}
- (())setBackgroundView:(id)view {
    todo_objc_setter!(this, view);
}

- (())layoutSubviews {
    let bounds: CGRect = msg![env; this bounds];
    let accessory_w = accessory_width(env, this);
    let host = env.objc.borrow::<UITableViewCellHostObject>(this);
    let content_view = host.content_view;
    let accessory_view = host.accessory_view;
    let text_label = host.text_label;
    let detail_label = host.detail_text_label;
    let image_view = host.image_view;
    let style = host.style;
    if content_view == nil {
        return;
    }

    let width = (bounds.size.width - accessory_w).max(0.0);
    let height = bounds.size.height;
    () = msg![env; content_view setFrame:(rect(0.0, 0.0, width, height))];

    if accessory_view != nil {
        let frame: CGRect = msg![env; accessory_view frame];
        let x = bounds.size.width - accessory_w + 10.0;
        let y = (height - frame.size.height) / 2.0;
        () = msg![env; accessory_view setFrame:(rect(x, y, frame.size.width, frame.size.height))];
    }

    // Image on the left
    let mut left = 10.0;
    if image_view != nil {
        let image: id = msg![env; image_view image];
        if image != nil {
            let size: CGSize = msg![env; image size];
            let y = ((height - size.height) / 2.0).max(0.0);
            () = msg![env; image_view setFrame:(rect(10.0, y, size.width, size.height))];
            left = 10.0 + size.width + 10.0;
        }
    }
    let right = 10.0;
    let avail = (width - left - right).max(0.0);

    match style {
        UITableViewCellStyleSubtitle => {
            set_label_frame(env, text_label, rect(left, 3.0, avail, 22.0));
            set_label_frame(env, detail_label, rect(left, 25.0, avail, 17.0));
        }
        UITableViewCellStyleValue1 => {
            // Detail text is right-aligned and takes as much room as it needs
            let mut detail_w = 0.0;
            if detail_label != nil {
                let text: id = msg![env; detail_label text];
                if text != nil {
                    let font: id = msg![env; detail_label font];
                    let size: CGSize = msg![env; text sizeWithFont:font];
                    detail_w = size.width.min(avail * 0.6).ceil() + 4.0;
                }
            }
            set_label_frame(env, text_label, rect(left, 0.0, (avail - detail_w).max(0.0), height));
            let detail_x = left + avail - detail_w;
            set_label_frame(env, detail_label, rect(detail_x, 0.0, detail_w, height));
        }
        UITableViewCellStyleValue2 => {
            set_label_frame(env, text_label, rect(10.0, 0.0, 74.0, height));
            set_label_frame(env, detail_label, rect(90.0, 0.0, (width - 100.0).max(0.0), height));
        }
        _ => {
            set_label_frame(env, text_label, rect(left, 0.0, avail, height));
        }
    }
}

- (())drawRect:(CGRect)_rect {
    let host = env.objc.borrow::<UITableViewCellHostObject>(this);
    let separator = host.separator_rgba;
    let accessory_type = host.accessory_type;
    let accessory_view = host.accessory_view;
    let on = (host.selected || host.highlighted)
        && host.selection_style != UITableViewCellSelectionStyleNone;
    let bounds: CGRect = msg![env; this bounds];
    let context = UIGraphicsGetCurrentContext(env);

    if let Some((r, g, b, a)) = separator {
        CGContextSetRGBFillColor(env, context, r, g, b, a);
        CGContextFillRect(
            env,
            context,
            rect(0.0, bounds.size.height - 1.0, bounds.size.width, 1.0),
        );
    }

    if accessory_view != nil {
        return;
    }
    let cy = bounds.size.height / 2.0;
    match accessory_type {
        UITableViewCellAccessoryDisclosureIndicator
        | UITableViewCellAccessoryDetailDisclosureButton => {
            // A ">" chevron built from small squares
            let (r, g, b) = if on { (1.0, 1.0, 1.0) } else { (0.55, 0.55, 0.57) };
            CGContextSetRGBFillColor(env, context, r, g, b, 1.0);
            let x0 = bounds.size.width - 20.0;
            for i in 0..6 {
                let i = i as CGFloat;
                fill_block(env, x0 + 1.3 * i, cy - 6.6 + 1.1 * i);
                fill_block(env, x0 + 1.3 * i, cy + 4.6 - 1.1 * i);
            }
        }
        UITableViewCellAccessoryCheckmark => {
            // A tick built from small squares
            let (r, g, b) = if on { (1.0, 1.0, 1.0) } else { LABEL_BLUE };
            CGContextSetRGBFillColor(env, context, r, g, b, 1.0);
            let x0 = bounds.size.width - 28.0;
            for i in 0..4 {
                let i = i as CGFloat;
                fill_block(env, x0 + 1.5 * i, cy + 1.5 * i - 2.0);
            }
            for i in 0..9 {
                let i = i as CGFloat;
                fill_block(env, x0 + 6.0 + 1.5 * i, cy + 4.0 - 1.4 * i);
            }
        }
        _ => {}
    }
}

// Touch handling: highlight while pressed, and report taps to the table view.

- (())touchesBegan:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    let touch: id = msg![env; touches anyObject];
    let location: CGPoint = msg![env; touch locationInView:this];
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).touch_start = Some(location);
    () = msg![env; this setHighlighted:true];
    msg_super![env; this touchesBegan:touches withEvent:event]
}

- (())touchesMoved:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    if let Some(start) = env.objc.borrow::<UITableViewCellHostObject>(this).touch_start {
        let touch: id = msg![env; touches anyObject];
        let location: CGPoint = msg![env; touch locationInView:this];
        let dx = location.x - start.x;
        let dy = location.y - start.y;
        if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
            // The user is scrolling (or dragging away), not tapping.
            env.objc.borrow_mut::<UITableViewCellHostObject>(this).touch_start = None;
            () = msg![env; this setHighlighted:false];
        }
    }
    msg_super![env; this touchesMoved:touches withEvent:event]
}

- (())touchesEnded:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    let tracking = env
        .objc
        .borrow_mut::<UITableViewCellHostObject>(this)
        .touch_start
        .take();
    let was_highlighted: bool = msg![env; this isHighlighted];
    if was_highlighted {
        () = msg![env; this setHighlighted:false];
    }
    if tracking.is_some() {
        let touch: id = msg![env; touches anyObject];
        let location: CGPoint = msg![env; touch locationInView:this];
        let inside: bool = msg![env; this pointInside:location withEvent:nil];
        let table: id = msg![env; this superview];
        if inside && table != nil {
            let sel = env
                .objc
                .register_host_selector("_touchHLE_cellTapped:".to_string(), &mut env.mem);
            let responds: bool = msg![env; table respondsToSelector:sel];
            if responds {
                () = msg![env; table _touchHLE_cellTapped:this];
            }
        }
    }
    msg_super![env; this touchesEnded:touches withEvent:event]
}

@end

};

/// Shared part of the initializers.
fn init_cell(env: &mut Environment, this: id, style: UITableViewCellStyle, reuse_id: id) -> id {
    // Cells are opaque white by default
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; this setBackgroundColor:white];

    let bounds: CGRect = msg![env; this bounds];
    let content_view: id = msg_class![env; UIView alloc];
    let content_view: id = msg![env; content_view initWithFrame:bounds];
    () = msg![env; content_view setOpaque:false];
    () = msg![env; this addSubview:content_view];

    retain(env, reuse_id);
    let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
    host.style = style;
    host.reuse_identifier = reuse_id;
    host.content_view = content_view;
    this
}

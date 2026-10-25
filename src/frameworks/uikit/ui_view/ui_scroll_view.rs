/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIScrollView`.

pub mod ui_text_view;
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    todo_objc_setter, ClassExports, NSZonePtr, SEL,
};
use crate::Environment;

type UIScrollViewIndicatorStyle = NSInteger;

pub struct UIScrollViewHostObject {
    superclass: super::UIViewHostObject,
    /// UIScrollViewDelegate, weak reference
    delegate: id,
    scroll_enabled: bool,
    content_offset: CGPoint,
    content_size: CGSize,
    indicator_style: UIScrollViewIndicatorStyle,
    shows_horizontal_indicator: bool,
    shows_vertical_indicator: bool,
    /// `UIView*`, owned, created when first needed
    vertical_indicator: id,
    /// `UIView*`, owned, created when first needed
    horizontal_indicator: id,
    /// The indicators are currently supposed to be visible (they are hidden
    /// again shortly after scrolling stops).
    indicators_visible: bool,
}
impl_HostObject_with_superclass!(UIScrollViewHostObject);
impl Default for UIScrollViewHostObject {
    fn default() -> Self {
        UIScrollViewHostObject {
            superclass: Default::default(),
            delegate: nil,
            scroll_enabled: true,
            content_offset: CGPoint { x: 0.0, y: 0.0 },
            content_size: CGSize {
                width: 0.0,
                height: 0.0,
            },
            indicator_style: 0, // UIScrollViewIndicatorStyleDefault
            shows_horizontal_indicator: true,
            shows_vertical_indicator: true,
            vertical_indicator: nil,
            horizontal_indicator: nil,
            indicators_visible: false,
        }
    }
}

/// Thickness of a scroll indicator.
const INDICATOR_THICKNESS: CGFloat = 3.0;
/// Space between a scroll indicator and the edge of the scroll view.
const INDICATOR_INSET: CGFloat = 2.0;
/// Shortest possible length of a scroll indicator.
const INDICATOR_MIN_LENGTH: CGFloat = 7.0;
/// How long the indicators stay visible after scrolling stops, in seconds.
const INDICATOR_LINGER: f64 = 0.5;

fn indicator_rect(x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize { width, height },
    }
}

fn indicator_color(env: &mut Environment, style: UIScrollViewIndicatorStyle) -> id {
    // UIScrollViewIndicatorStyleWhite is 2, the default and black styles are
    // both dark.
    let brightness: CGFloat = if style == 2 { 1.0 } else { 0.0 };
    msg_class![env; UIColor colorWithRed:brightness green:brightness blue:brightness alpha:0.5f32]
}

/// Make a (hidden) indicator view and add it to the scroll view.
fn make_indicator(env: &mut Environment, scroll_view: id) -> id {
    let view: id = msg_class![env; UIView alloc];
    let view: id = msg![env; view initWithFrame:(indicator_rect(0.0, 0.0, 0.0, 0.0))];
    () = msg![env; view setUserInteractionEnabled:false];
    let style = env
        .objc
        .borrow::<UIScrollViewHostObject>(scroll_view)
        .indicator_style;
    let color = indicator_color(env, style);
    () = msg![env; view setBackgroundColor:color];
    let layer: id = msg![env; view layer];
    () = msg![env; layer setCornerRadius:(INDICATOR_THICKNESS / 2.0)];
    () = msg![env; view setAlpha:0.0f32];
    () = msg![env; scroll_view addSubview:view];
    view
}

/// Move and resize the indicators to match the scroll position, and show or
/// hide them as appropriate.
fn update_indicators(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UIScrollViewHostObject>(this);
    let vertical = host.vertical_indicator;
    let horizontal = host.horizontal_indicator;
    if vertical == nil && horizontal == nil {
        return;
    }
    let content = host.content_size;
    let shows_vertical = host.shows_vertical_indicator;
    let shows_horizontal = host.shows_horizontal_indicator;
    let alpha: f32 = if host.indicators_visible { 1.0 } else { 0.0 };
    let bounds: CGRect = msg![env; this bounds];

    let can_scroll_vertically = shows_vertical && content.height > bounds.size.height;
    let can_scroll_horizontally = shows_horizontal && content.width > bounds.size.width;
    // Leave room in the corner if both indicators are shown.
    let corner = if can_scroll_vertically && can_scroll_horizontally {
        INDICATOR_THICKNESS + INDICATOR_INSET
    } else {
        0.0
    };

    if vertical != nil {
        if can_scroll_vertically {
            let track = (bounds.size.height - 2.0 * INDICATOR_INSET - corner).max(0.0);
            let length = (track * bounds.size.height / content.height)
                .clamp(INDICATOR_MIN_LENGTH.min(track), track);
            let max_offset = content.height - bounds.size.height;
            let fraction = (bounds.origin.y / max_offset).clamp(0.0, 1.0);
            let x = bounds.origin.x + bounds.size.width - INDICATOR_INSET - INDICATOR_THICKNESS;
            let y = bounds.origin.y + INDICATOR_INSET + (track - length) * fraction;
            let frame = indicator_rect(x, y, INDICATOR_THICKNESS, length);
            () = msg![env; vertical setFrame:frame];
        }
        let vertical_alpha = if can_scroll_vertically { alpha } else { 0.0 };
        () = msg![env; vertical setAlpha:vertical_alpha];
    }
    if horizontal != nil {
        if can_scroll_horizontally {
            let track = (bounds.size.width - 2.0 * INDICATOR_INSET - corner).max(0.0);
            let length = (track * bounds.size.width / content.width)
                .clamp(INDICATOR_MIN_LENGTH.min(track), track);
            let max_offset = content.width - bounds.size.width;
            let fraction = (bounds.origin.x / max_offset).clamp(0.0, 1.0);
            let x = bounds.origin.x + INDICATOR_INSET + (track - length) * fraction;
            let y = bounds.origin.y + bounds.size.height - INDICATOR_INSET - INDICATOR_THICKNESS;
            let frame = indicator_rect(x, y, length, INDICATOR_THICKNESS);
            () = msg![env; horizontal setFrame:frame];
        }
        let horizontal_alpha = if can_scroll_horizontally { alpha } else { 0.0 };
        () = msg![env; horizontal setAlpha:horizontal_alpha];
    }
}

fn hide_selector(env: &mut Environment) -> SEL {
    env.objc
        .register_host_selector("_touchHLE_hideScrollIndicators".to_string(), &mut env.mem)
}

/// Show the indicators, creating them if needed, and keep them there until
/// [schedule_hide_indicators] is called.
fn show_indicators(env: &mut Environment, this: id) {
    let host = env.objc.borrow::<UIScrollViewHostObject>(this);
    let mut vertical = host.vertical_indicator;
    let mut horizontal = host.horizontal_indicator;
    let content = host.content_size;
    let shows_vertical = host.shows_vertical_indicator;
    let shows_horizontal = host.shows_horizontal_indicator;
    let bounds: CGRect = msg![env; this bounds];

    if vertical == nil && shows_vertical && content.height > bounds.size.height {
        vertical = make_indicator(env, this);
        env.objc
            .borrow_mut::<UIScrollViewHostObject>(this)
            .vertical_indicator = vertical;
    }
    if horizontal == nil && shows_horizontal && content.width > bounds.size.width {
        horizontal = make_indicator(env, this);
        env.objc
            .borrow_mut::<UIScrollViewHostObject>(this)
            .horizontal_indicator = horizontal;
    }

    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .indicators_visible = true;
    update_indicators(env, this);

    // Subviews added later (e.g. table cells) must not cover the indicators.
    for indicator in [vertical, horizontal] {
        if indicator != nil {
            () = msg![env; this bringSubviewToFront:indicator];
        }
    }

    let sel = hide_selector(env);
    () = msg_class![env; NSObject cancelPreviousPerformRequestsWithTarget:this selector:sel object:nil];
}

/// Hide the indicators a short while from now.
fn schedule_hide_indicators(env: &mut Environment, this: id) {
    let sel = hide_selector(env);
    () = msg_class![env; NSObject cancelPreviousPerformRequestsWithTarget:this selector:sel object:nil];
    () = msg![env; this performSelector:sel withObject:nil afterDelay:INDICATOR_LINGER];
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIScrollView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIScrollViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let &UIScrollViewHostObject {
        vertical_indicator,
        horizontal_indicator,
        ..
    } = env.objc.borrow(this);
    // The indicators are also subviews, which UIView's dealloc will release.
    release(env, vertical_indicator);
    release(env, horizontal_indicator);
    msg_super![env; this dealloc]
}

- (id)delegate {
    env.objc.borrow::<UIScrollViewHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).delegate = delegate;
}

- (())setDelaysContentTouches:(id)_delay_content_touches{
    // TODO
}
- (())setBounces:(id)_bounces {
    // TODO
}

- (bool)scrollEnabled {
    env.objc.borrow::<UIScrollViewHostObject>(this).scroll_enabled
}
- (())setScrollEnabled:(bool)scroll_enabled {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).scroll_enabled = scroll_enabled;
}

- (())setDirectionalLockEnabled:(bool)enabled {
    todo_objc_setter!(this, enabled);
}

- (())setPagingEnabled:(bool)enabled {
    todo_objc_setter!(this, enabled);
}

- (bool)showsHorizontalScrollIndicator {
    env.objc.borrow::<UIScrollViewHostObject>(this).shows_horizontal_indicator
}
- (())setShowsHorizontalScrollIndicator:(bool)value {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).shows_horizontal_indicator = value;
    update_indicators(env, this);
}

- (bool)showsVerticalScrollIndicator {
    env.objc.borrow::<UIScrollViewHostObject>(this).shows_vertical_indicator
}
- (())setShowsVerticalScrollIndicator:(bool)value {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).shows_vertical_indicator = value;
    update_indicators(env, this);
}

- (())setScrollsToTop:(bool)value {
    todo_objc_setter!(this, value);
}

- (CGPoint)contentOffset {
    env.objc.borrow::<UIScrollViewHostObject>(this).content_offset
}
- (())setContentOffset:(CGPoint)offset {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).content_offset = offset;
    // Bounds origin should be equals to the content offset
    let mut bounds: CGRect = msg![env; this bounds];
    bounds.origin = offset;
    () = msg![env; this setBounds:bounds];
    () = msg![env; this setNeedsDisplay];
    update_indicators(env, this);
}

- (CGSize)contentSize {
    env.objc.borrow::<UIScrollViewHostObject>(this).content_size
}
- (())setContentSize:(CGSize)size {
    env.objc.borrow_mut::<UIScrollViewHostObject>(this).content_size = size;
    update_indicators(env, this);
}

- (UIScrollViewIndicatorStyle)indicatorStyle {
    env.objc.borrow::<UIScrollViewHostObject>(this).indicator_style
}
- (())setIndicatorStyle:(UIScrollViewIndicatorStyle)style {
    let host = env.objc.borrow_mut::<UIScrollViewHostObject>(this);
    host.indicator_style = style;
    let indicators = [host.vertical_indicator, host.horizontal_indicator];
    let color = indicator_color(env, style);
    for indicator in indicators {
        if indicator != nil {
            () = msg![env; indicator setBackgroundColor:color];
        }
    }
}

// Briefly show the scroll indicators.
- (())flashScrollIndicators {
    show_indicators(env, this);
    schedule_hide_indicators(env, this);
}

- (())_touchHLE_hideScrollIndicators {
    env.objc
        .borrow_mut::<UIScrollViewHostObject>(this)
        .indicators_visible = false;
    update_indicators(env, this);
}

- (())touchesMoved:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let scroll_enabled: bool = msg![env; this scrollEnabled];
    if !scroll_enabled {
        return;
    }

    let touch_arr: id = msg![env; touches allObjects];
    // Assume single finger touches for now
    let touch: id = msg![env; touch_arr objectAtIndex:0u32];
    let bounds: CGRect = msg![env; this bounds];

    let prev_location: CGPoint = msg![env; touch previousLocationInView:this];
    let prev_x = prev_location.x;
    let prev_y = prev_location.y;

    let new_location: CGPoint = msg![env; touch locationInView:this];
    let y = new_location.y;
    let x = new_location.x;

    let delta_y = y - prev_y;
    let delta_x = x - prev_x;

    let offset: CGPoint = msg![env; this contentOffset];
    let content_size: CGSize = msg![env; this contentSize];

    // Very rudimentary scrolling.
    // We emulate sliding up to scroll down like on the real iPhone.
    let mut new_content_offset: CGPoint = CGPoint { x: offset.x - delta_x, y: offset.y - delta_y };

    // Update content offset within bounds
    new_content_offset.y = new_content_offset.y.min(content_size.height - bounds.size.height).max(0.0);
    new_content_offset.x = new_content_offset.x.min(content_size.width - bounds.size.width).max(0.0);

    // Trigger rerender only if required.
    log_dbg!("content offset: old {:?}, new {:?}", offset, new_content_offset);
    if new_content_offset != offset {
        () = msg![env; this setContentOffset:new_content_offset];
        show_indicators(env, this);

        let delegate: id = msg![env; this delegate];
        let sel: SEL = env
            .objc
            .register_host_selector("scrollViewDidScroll:".to_string(), &mut env.mem);
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate scrollViewDidScroll:this];
        }
    }
}

- (())touchesEnded:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    schedule_hide_indicators(env, this);
    msg_super![env; this touchesEnded:touches withEvent:event]
}

@end

};

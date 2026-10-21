/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `MPMoviePlayerController` etc.

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::foundation::{ns_string, ns_url, NSInteger, NSTimeInterval};
use crate::frameworks::uikit::ui_device::UIDeviceOrientation;
use crate::mem::{MutPtr, Ptr};
use crate::objc::{
    id, msg, msg_class, nil, objc_classes, release, retain, todo_objc_setter, ClassExports,
    HostObject, NSZonePtr,
};
use crate::Environment;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct State {
    active_player: Option<id>,
    /// Various apps (e.g. Crash Bandicoot Nitro Kart 3D and Spore Origins)
    /// create or start a player and await some kind of notification, but can't
    /// handle it if that notification happens immediately. This queue lets us
    /// delay such notifications until the app next returns to the run loop,
    /// which seems to be late enough.
    pending_notifications: VecDeque<(&'static str, id, Instant)>,
}
impl State {
    fn get(env: &mut Environment) -> &mut Self {
        &mut env.framework_state.media_player.movie_player
    }
}

type MPMovieScalingMode = NSInteger;
type MPMovieControlStyle = NSInteger;

type MPMoviePlaybackState = NSInteger;
const MPMoviePlaybackStateStopped: MPMoviePlaybackState = 0;

// Values might not be correct, but as these are linked symbol constants, it
// shouldn't matter.
pub const MPMoviePlayerPlaybackDidFinishNotification: &str =
    "MPMoviePlayerPlaybackDidFinishNotification";
/// Apparently an undocumented, private API. Spore Origins uses it.
pub const MPMoviePlayerContentPreloadDidFinishNotification: &str =
    "MPMoviePlayerContentPreloadDidFinishNotification";
pub const MPMoviePlayerScalingModeDidChangeNotification: &str =
    "MPMoviePlayerScalingModeDidChangeNotification";
// TODO: More notifications?
const MPMoviePlayerPlaybackDidFinishReasonUserInfoKey: &str =
    "MPMoviePlayerPlaybackDidFinishReasonUserInfoKey";

/// `NSNotificationName` values and other constants.
pub const CONSTANTS: ConstantExports = &[
    (
        "_MPMoviePlayerPlaybackDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishNotification),
    ),
    (
        "_MPMoviePlayerContentPreloadDidFinishNotification",
        HostConstant::NSString(MPMoviePlayerContentPreloadDidFinishNotification),
    ),
    (
        "_MPMoviePlayerScalingModeDidChangeNotification",
        HostConstant::NSString(MPMoviePlayerScalingModeDidChangeNotification),
    ),
    (
        "_MPMoviePlayerPlaybackDidFinishReasonUserInfoKey",
        HostConstant::NSString(MPMoviePlayerPlaybackDidFinishReasonUserInfoKey),
    ),
];

struct MPMoviePlayerControllerHostObject {
    // NSURL *
    content_url: id,
    /// `AVAudioPlayer*` playing the movie's audio track, or nil. There is no
    /// video decoder yet, so only the sound is played.
    audio_player: id,
}
impl HostObject for MPMoviePlayerControllerHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation MPMoviePlayerController: NSObject

// TODO: actual playback

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(MPMoviePlayerControllerHostObject {
        content_url: nil,
        audio_player: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithContentURL:(id)url { // NSURL*
    log!(
        "TODO: [(MPMoviePlayerController*){:?} initWithContentURL:{:?} ({:?})]",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );

    retain(env, url);
    env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).content_url = url;

    // Act as if loading immediately completed (Spore Origins waits for this).
    // The player is retained until the notification has been posted.
    retain(env, this);
    State::get(env).pending_notifications.push_back(
        (MPMoviePlayerContentPreloadDidFinishNotification, this, Instant::now())
    );

    this
}

- (())dealloc {
    let &MPMoviePlayerControllerHostObject { content_url, audio_player } = env.objc.borrow(this);
    release(env, audio_player); // stops playback
    release(env, content_url);

    env.objc.dealloc_object(this, &mut env.mem);
}

- (id)contentURL {
    env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).content_url
}

- (id)backgroundColor {
    msg_class![env; UIColor blackColor] // TODO
}
- (())setBackgroundColor:(id)color { // UIColor*
    todo_objc_setter!(this, color);
}

- (())setScalingMode:(MPMovieScalingMode)mode {
    todo_objc_setter!(this, mode);
}
- (())setUseApplicationAudioSession:(bool)use_session {
    todo_objc_setter!(this, use_session);
}
- (())setControlStyle:(MPMovieControlStyle)style {
    todo_objc_setter!(this, style);
}
- (())setFullscreen:(bool)fullsreen {
    todo_objc_setter!(this, fullsreen);
}
- (())setInitialPlaybackTime:(NSTimeInterval)initial_time {
    todo_objc_setter!(this, initial_time);
}

- (id)view {
    nil // TODO
}

- (MPMoviePlaybackState)playbackState {
    MPMoviePlaybackStateStopped // TODO
}

// Apparently an undocumented, private API, but Spore Origins uses it.
- (())setMovieControlMode:(NSInteger)_mode {
    // As this is undocumented and we don't have real video playback yet, let's
    // ignore it.
}

// Another undocumented one! But some apps may still use it :/
// https://stackoverflow.com/a/1390079/2241008
- (())setOrientation:(UIDeviceOrientation)_orientation animated:(bool)_animated {

}

// MPMediaPlayback implementation
- (())play {
    log!("TODO: [(MPMoviePlayerController*){:?} play]", this);
    // Some apps (e.g. NOVA2) call `play` twice on the same player, which
    // shouldn't restart or stop anything.
    let already_active = env.framework_state.media_player.movie_player.active_player == Some(this);
    if !already_active {
        if let Some(old) = env.framework_state.media_player.movie_player.active_player {
            let _: () = msg![env; old stop];
        }
        assert!(env.framework_state.media_player.movie_player.active_player.is_none());
        // Movie player is retained by the runtime until it is stopped
        retain(env, this);
        env.framework_state.media_player.movie_player.active_player = Some(this);
    }

    // There is no video decoding yet, but we can play the audio track and use
    // its length as the length of the movie. If the audio can't be played,
    // act as if playback immediately completed after 1 second
    // (various apps wait for this, such as BIA and Hero of Sparta).
    let delay = match start_audio(env, this) {
        Some(seconds) => Duration::from_secs_f64(seconds),
        None => Duration::from_millis(1000),
    };
    let notif = (MPMoviePlayerPlaybackDidFinishNotification, this, Instant::now().checked_add(delay).unwrap());
    for (name, obj, _) in &mut State::get(env).pending_notifications {
        // De-duplicate similar notifications. This can happen if app is calling
        // `play` twice on the same player object (case of NOVA2).
        if *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == this {
            return;
        }
    }
    // The player is retained until the notification has been posted.
    retain(env, this);
    State::get(env).pending_notifications.push_back(notif);
}

- (())pause {
    log!("TODO: [(MPMoviePlayerController*){:?} pause]", this);
}

- (())stop {
    log!("TODO: [(MPMoviePlayerController*){:?} stop]", this);

    // Stop the audio.
    let audio_player = env.objc.borrow::<MPMoviePlayerControllerHostObject>(this).audio_player;
    if audio_player != nil {
        env.objc.borrow_mut::<MPMoviePlayerControllerHostObject>(this).audio_player = nil;
        release(env, audio_player); // stops playback
    }

    // The movie is over now, so if the "playback finished" notification is
    // still waiting for the movie's end, send it right away rather than
    // (much) later.
    for (name, obj, time) in &mut State::get(env).pending_notifications {
        if *name == MPMoviePlayerPlaybackDidFinishNotification && *obj == this {
            *time = Instant::now();
        }
    }

    // Some applications (like NOVA2) may send 2 `stop` messages for each
    // 1 `play` message for the player, and an app may also stop a player that
    // is no longer the active one. We only want to release the active player
    // once.
    if env.framework_state.media_player.movie_player.active_player == Some(this) {
        env.framework_state.media_player.movie_player.active_player = None;
        release(env, this);
    }
}

@end

@implementation MPMoviePlayerViewController: UIViewController

- (id)initWithContentURL:(id)url {
    log!(
        "TODO: [(MPMoviePlayerViewController*){:?} initWithContentURL:{:?} ({:?})] -> nil",
        this,
        url,
        ns_url::to_rust_path(env, url),
    );
    release(env, this);
    nil // TODO
}

@end

};

/// Start playing the audio track of the movie at `content_url`, if it can be
/// decoded. Returns its duration in seconds.
fn start_audio(env: &mut Environment, player: id) -> Option<NSTimeInterval> {
    let &MPMoviePlayerControllerHostObject {
        content_url,
        audio_player,
    } = env.objc.borrow(player);
    if content_url == nil {
        return None;
    }

    // `play` was called again on a player that is already playing audio.
    if audio_player != nil {
        let duration: NSTimeInterval = msg![env; audio_player duration];
        return Some(duration);
    }

    let no_error: MutPtr<id> = Ptr::null();
    let audio_player: id = msg_class![env; AVAudioPlayer alloc];
    let audio_player: id = msg![env; audio_player initWithContentsOfURL:content_url error:no_error];
    if audio_player == nil {
        log!("Couldn't decode the audio of the movie, it will play silently.");
        return None;
    }

    let duration: NSTimeInterval = msg![env; audio_player duration];
    if !duration.is_finite() || duration <= 0.0 {
        release(env, audio_player);
        return None;
    }
    // Don't trust absurd durations.
    let duration = duration.min(6.0 * 60.0 * 60.0);

    let _: bool = msg![env; audio_player play];
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player)
        .audio_player = audio_player;
    log!(
        "Playing the audio of the movie ({:.2} seconds), the video is not shown yet.",
        duration
    );
    Some(duration)
}

/// For use by `NSRunLoop` via [super::handle_players]: check movie players'
/// status, send notifications if necessary.
pub(super) fn handle_players(env: &mut Environment) {
    let mut notifs_to_run = Vec::new();
    let pending_notifs = &mut State::get(env).pending_notifications;
    let mut i = 0;
    while i < pending_notifs.len() {
        let (name_str, object, time) = pending_notifs[i];
        if Instant::now() >= time {
            notifs_to_run.push((name_str, object));
            pending_notifs.swap_remove_back(i);
        } else {
            i += 1;
        }
    }
    for (name_str, object) in notifs_to_run {
        let name = ns_string::get_static_str(env, name_str);
        let center: id = msg_class![env; NSNotificationCenter defaultCenter];
        // TODO: should there be some user info attached?
        let _: () = msg![env; center postNotificationName:name object:object];
        // Balances the retain done when the notification was queued.
        release(env, object);
    }
}

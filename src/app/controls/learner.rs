//! What the Manage window waits for from the device: the control of an action, the button of a
//! layer, or the clicks of an encoder being detected. One thing at a time, and `feed` says what a
//! message from the device was taken for; the window does the saving.

use std::collections::BTreeMap;

use crate::controller::{self, Action, Encoding, Learned, Msg, Reading, Src};

/// How many clicks to each side tell an encoder's encoding.
const DETECT_CLICKS: usize = 4;

/// The actions in the order the editor lists them.
pub(super) fn action_order() -> Vec<Action> {
    Action::sections().into_iter().flat_map(|(_, actions)| actions).collect()
}

/// The values an encoder sent while its encoding is being found out.
struct Detect {
    action: Action,
    /// The cc and channel of the first message; others are ignored.
    cc: Option<(u8, u8)>,
    clockwise: Vec<u8>,
    counter: Vec<u8>,
}

enum Waiting {
    Nothing,
    /// The control for an action.
    Control(Action),
    /// The button that selects a layer (the name it will have).
    Layer(String),
    /// An encoder is being turned right and then left to find out how it counts.
    Detect(Detect),
}

/// What a message from the device was taken for.
#[derive(Debug, PartialEq)]
pub(super) enum Heard {
    /// Nothing waits for a control: the message is for the controller to act on.
    Unused,
    /// Used up, and changes nothing (a release, a fader for a button, a click that is not counted).
    Ignored,
    /// `found` is the control of `action`; it is to be saved, then `bound` or `not_bound` said.
    Control { action: Action, found: Learned },
    /// `src` is the button that selects the layer `name`.
    Layer { name: String, src: Src },
    /// Enough clicks in both directions; `encoding` is the one that fits, if one does.
    Detected { action: Action, encoding: Option<Encoding>, clockwise: Vec<u8>, counter: Vec<u8> },
}

pub(super) struct Learner {
    waiting: Waiting,
    /// After a control is learned, wait for one for the next action in the list.
    pub(super) keep_going: bool,
    /// Limit a learned control to the MIDI channel it sent on.
    pub(super) remember_channel: bool,
    /// How a value's cc is read, chosen before it is learned (by the action's name).
    kinds: BTreeMap<Action, Reading>,
}

impl Learner {
    pub(super) fn new() -> Learner {
        Learner { waiting: Waiting::Nothing, keep_going: true, remember_channel: false, kinds: BTreeMap::new() }
    }

    /// Waits for the control of `action`, instead of what it waited for.
    pub(super) fn wait_for_control(&mut self, action: Action) {
        self.waiting = Waiting::Control(action);
    }

    /// Waits for the button of the layer to be called `name`.
    pub(super) fn wait_for_layer(&mut self, name: String) {
        self.waiting = Waiting::Layer(name);
    }

    /// Waits for the clicks of the encoder of `action`.
    pub(super) fn wait_for_clicks(&mut self, action: Action) {
        self.waiting = Waiting::Detect(Detect { action, cc: None, clockwise: Vec::new(), counter: Vec::new() });
    }

    pub(super) fn stop(&mut self) {
        self.waiting = Waiting::Nothing;
    }

    /// The action that waits for its control.
    pub(super) fn control_waited_for(&self) -> Option<Action> {
        match self.waiting {
            Waiting::Control(action) => Some(action),
            _ => None,
        }
    }

    /// The name of the layer that waits for its button.
    pub(super) fn layer_waited_for(&self) -> Option<&str> {
        match &self.waiting {
            Waiting::Layer(name) => Some(name),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn is_detecting(&self) -> bool {
        matches!(self.waiting, Waiting::Detect(_))
    }

    /// What to ask of the user while the clicks are counted: right first, then left.
    pub(super) fn prompt(&self) -> Option<String> {
        let Waiting::Detect(d) = &self.waiting else { return None };
        Some(if d.clockwise.len() < DETECT_CLICKS {
            format!(
                "Turn the encoder for \"{}\" slowly to the RIGHT, {} clicks ({} so far)…",
                d.action.title(),
                DETECT_CLICKS,
                d.clockwise.len()
            )
        } else {
            format!("Now slowly to the LEFT, {} clicks ({} so far)…", DETECT_CLICKS, d.counter.len())
        })
    }

    /// How a value's cc is read for `action` when nothing is bound yet: the choice made in the
    /// table, or (absolute, except for the lists, which are only turned).
    pub(super) fn chosen_kind(&self, action: Action) -> Reading {
        self.kinds.get(&action).copied().unwrap_or(if action.is_browse() { Reading::Relative(Encoding::Mackie) } else { Reading::Absolute })
    }

    pub(super) fn choose_kind(&mut self, action: Action, reading: Reading) {
        self.kinds.insert(action, reading);
    }

    /// The control of the waiting action was saved: waits for the next one in the list, if asked to.
    pub(super) fn bound(&mut self, action: Action) {
        self.waiting = Waiting::Nothing;
        if self.keep_going {
            let order = action_order();
            if let Some(next) = order.iter().position(|a| *a == action).and_then(|i| order.get(i + 1)) {
                self.waiting = Waiting::Control(*next);
            }
        }
    }

    /// Takes a message from the device for what waits for it. `existing` is how the preset reads
    /// the cc already bound to an action, which a new control for it keeps reading the same way.
    pub(super) fn feed(&mut self, msg: Msg, existing: impl FnOnce(Action) -> Option<Reading>) -> Heard {
        match &mut self.waiting {
            Waiting::Nothing => Heard::Unused,
            Waiting::Detect(d) => {
                let Msg::Cc { ch, cc, val } = msg else { return Heard::Ignored };
                if *d.cc.get_or_insert((cc, ch)) != (cc, ch) || val == 0 {
                    return Heard::Ignored;
                }
                if d.clockwise.len() < DETECT_CLICKS {
                    d.clockwise.push(val);
                } else {
                    d.counter.push(val);
                }
                if d.counter.len() < DETECT_CLICKS {
                    return Heard::Ignored;
                }
                let Waiting::Detect(d) = std::mem::replace(&mut self.waiting, Waiting::Nothing) else { unreachable!() };
                let encoding = Encoding::guess(&d.clockwise, &d.counter);
                if let Some(encoding) = encoding {
                    self.choose_kind(d.action, Reading::Relative(encoding));
                }
                Heard::Detected { action: d.action, encoding, clockwise: d.clockwise, counter: d.counter }
            }
            // Any button press will do; the action only decides what kind of message counts.
            Waiting::Layer(name) => match controller::learn(msg, Action::Stop, Reading::Absolute, false) {
                Some(found) => Heard::Layer { name: name.clone(), src: found.input.src },
                None => Heard::Ignored,
            },
            Waiting::Control(action) => {
                let action = *action;
                let reading = existing(action).unwrap_or_else(|| self.chosen_kind(action));
                match controller::learn(msg, action, reading, self.remember_channel) {
                    Some(found) => Heard::Control { action, found },
                    None => Heard::Ignored,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc(ch: u8, cc: u8, val: u8) -> Msg {
        Msg::Cc { ch, cc, val }
    }

    fn pressed(note: u8) -> Msg {
        Msg::NoteOn { ch: 1, note, vel: 100 }
    }

    fn heard_control(h: Heard) -> (Action, Src) {
        match h {
            Heard::Control { action, found } => (action, found.input.src),
            other => panic!("not a control: {other:?}"),
        }
    }

    #[test]
    fn nothing_waits_at_the_start_and_a_message_is_not_used_up() {
        let mut l = Learner::new();
        assert_eq!(l.feed(pressed(36), |_| None), Heard::Unused);
        assert!(l.control_waited_for().is_none() && l.layer_waited_for().is_none() && !l.is_detecting());
    }

    #[test]
    fn a_control_is_heard_once_and_the_next_action_is_waited_for_when_asked() {
        let mut l = Learner::new();
        l.wait_for_control(Action::Record);
        assert_eq!(l.feed(Msg::NoteOn { ch: 1, note: 36, vel: 0 }, |_| None), Heard::Ignored, "a release is not a press");
        assert_eq!(l.control_waited_for(), Some(Action::Record));
        assert_eq!(heard_control(l.feed(pressed(36), |_| None)), (Action::Record, Src::Note(36)));
        l.bound(Action::Record);
        let order = action_order();
        assert_eq!(l.control_waited_for(), Some(order[1]));

        l.keep_going = false;
        l.bound(order[1]);
        assert_eq!(l.control_waited_for(), None, "it ends after one control");
        l.keep_going = true;
        l.bound(*order.last().unwrap());
        assert_eq!(l.control_waited_for(), None, "and after the last action there is no next");
    }

    #[test]
    fn a_value_is_read_as_the_preset_reads_it_or_as_chosen() {
        let mut l = Learner::new();
        l.wait_for_control(Action::Tempo);
        let Heard::Control { found, .. } = l.feed(cc(1, 13, 40), |_| Some(Reading::Relative(Encoding::Twos))) else { panic!() };
        assert_eq!(found.reading, Reading::Relative(Encoding::Twos));
        l.choose_kind(Action::Tempo, Reading::Relative(Encoding::Offset));
        let Heard::Control { found, .. } = l.feed(cc(1, 13, 40), |_| None) else { panic!() };
        assert_eq!(found.reading, Reading::Relative(Encoding::Offset));
        assert_eq!(Learner::new().chosen_kind(Action::BrowseScores), Reading::Relative(Encoding::Mackie), "a list is only turned");
        assert_eq!(Learner::new().chosen_kind(Action::Tempo), Reading::Absolute);
    }

    #[test]
    fn the_channel_is_kept_when_asked() {
        let mut l = Learner::new();
        l.remember_channel = true;
        l.wait_for_control(Action::Stop);
        let Heard::Control { found, .. } = l.feed(Msg::NoteOn { ch: 3, note: 40, vel: 90 }, |_| None) else { panic!() };
        assert_eq!(found.input.channel, Some(3));
    }

    #[test]
    fn a_layer_waits_for_a_press_and_not_for_a_fader() {
        let mut l = Learner::new();
        l.wait_for_layer("B".into());
        assert_eq!(l.layer_waited_for(), Some("B"));
        assert_eq!(l.feed(Msg::Bend { ch: 1, value: 100 }, |_| None), Heard::Ignored);
        assert_eq!(l.feed(pressed(85), |_| None), Heard::Layer { name: "B".into(), src: Src::Note(85) });
        assert_eq!(l.layer_waited_for(), Some("B"), "it waits until the window has saved it");
    }

    #[test]
    fn an_encoder_is_turned_right_and_left_and_its_encoding_is_chosen() {
        let mut l = Learner::new();
        l.wait_for_clicks(Action::Tempo);
        assert!(l.prompt().unwrap().contains("RIGHT"));
        assert_eq!(l.feed(pressed(36), |_| None), Heard::Ignored, "a button is no encoder");
        for v in [1, 1, 1] {
            assert_eq!(l.feed(cc(1, 13, v), |_| None), Heard::Ignored);
        }
        assert_eq!(l.feed(cc(1, 14, 99), |_| None), Heard::Ignored, "another knob does not count");
        assert_eq!(l.feed(cc(1, 13, 0), |_| None), Heard::Ignored, "nor does a zero");
        assert_eq!(l.feed(cc(1, 13, 1), |_| None), Heard::Ignored);
        assert!(l.prompt().unwrap().contains("LEFT"));
        for v in [65, 65, 65] {
            assert_eq!(l.feed(cc(1, 13, v), |_| None), Heard::Ignored);
        }
        let Heard::Detected { action, encoding, clockwise, counter } = l.feed(cc(1, 13, 65), |_| None) else { panic!() };
        assert_eq!((action, encoding), (Action::Tempo, Some(Encoding::Mackie)));
        assert_eq!((clockwise, counter), (vec![1; 4], vec![65; 4]));
        assert!(!l.is_detecting() && l.prompt().is_none());
        assert_eq!(l.chosen_kind(Action::Tempo), Reading::Relative(Encoding::Mackie));
    }

    #[test]
    fn an_encoder_that_sends_positions_has_no_encoding() {
        let mut l = Learner::new();
        l.wait_for_clicks(Action::Tempo);
        let mut last = Heard::Ignored;
        for v in [10, 20, 30, 40, 35, 25, 15, 5] {
            last = l.feed(cc(1, 13, v), |_| None);
        }
        let Heard::Detected { encoding, .. } = last else { panic!("{last:?}") };
        assert_eq!(encoding, None);
        assert_eq!(l.chosen_kind(Action::Tempo), Reading::Absolute, "nothing was chosen");
    }

    #[test]
    fn waiting_for_one_thing_ends_waiting_for_another() {
        let mut l = Learner::new();
        l.wait_for_control(Action::Record);
        l.wait_for_clicks(Action::Tempo);
        assert!(l.control_waited_for().is_none() && l.is_detecting());
        l.wait_for_layer("B".into());
        assert!(!l.is_detecting() && l.layer_waited_for().is_some());
        l.stop();
        assert_eq!(l.feed(pressed(1), |_| None), Heard::Unused);
    }
}

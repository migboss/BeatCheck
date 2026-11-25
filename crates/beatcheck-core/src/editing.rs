use crate::model::{BeatMark, GridMode, MarkKind, TimeSignature, TrackAnalysis};

/// Undo / Redo snapshot stack for non-destructive mark & grid editing.
#[derive(Debug, Clone, Default)]
pub struct EditHistory {
    undo_stack: Vec<TrackAnalysis>,
    redo_stack: Vec<TrackAnalysis>,
    max_depth: usize,
}

impl EditHistory {
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: max_depth.max(8),
        }
    }

    pub fn push_snapshot(&mut self, state: &TrackAnalysis) {
        self.undo_stack.push(state.clone());
        if self.undo_stack.len() > self.max_depth {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo(&mut self, current: &mut TrackAnalysis) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(current.clone());
            *current = prev;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, current: &mut TrackAnalysis) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(current.clone());
            *current = next;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

/// Regenerate beat & drop marks cleanly from raw detected timings + active grid settings.
pub fn rebuild_marks_from_raw(track: &mut TrackAnalysis) {
    let bpb = track.active_time_sig.beats_per_bar() as usize;
    // Fix 1: Keep exact sub-millisecond BPM precision (UI formats to 1 decimal place cleanly)
    let bpm = (track.detected_bpm * track.bpm_multiplier).clamp(35.0, 260.0);
    track.active_bpm = bpm;
    let period = 60.0 / track.active_bpm;

    // 1. Build beat sequence according to GridMode and bpm_multiplier
    let beat_times: Vec<f64> = match track.grid_mode {
        GridMode::LockedConstantBpm => {
            let anchor = find_best_grid_anchor(
                &track.raw_beat_times,
                &track.raw_downbeat_times,
                period,
            );
            let mut times = Vec::new();
            let first_k = if track.extrapolate_silence {
                -(anchor / period).floor() as i64
            } else {
                let first_raw = track.raw_beat_times.first().copied().unwrap_or(0.0);
                ((first_raw - anchor) / period).round() as i64
            };
            let last_t = if track.extrapolate_silence {
                track.duration_sec
            } else {
                track
                    .raw_beat_times
                    .last()
                    .copied()
                    .unwrap_or(track.duration_sec)
            };

            let mut k = first_k;
            loop {
                let t = anchor + (k as f64) * period;
                if t > last_t + 1e-4 || t > track.duration_sec + 1e-4 {
                    break;
                }
                if t >= -1e-4 {
                    times.push(t.max(0.0));
                }
                k += 1;
            }
            times
        }
        GridMode::Dynamic => {
            if track.raw_beat_times.is_empty() {
                vec![]
            } else if (track.bpm_multiplier - 0.5).abs() < 0.05 {
                track.raw_beat_times.iter().step_by(2).copied().collect()
            } else if (track.bpm_multiplier - 2.0).abs() < 0.05 {
                let mut doubled = Vec::with_capacity(track.raw_beat_times.len() * 2);
                for w in track.raw_beat_times.windows(2) {
                    doubled.push(w[0]);
                    doubled.push(0.5 * (w[0] + w[1]));
                }
                if let Some(&last) = track.raw_beat_times.last() {
                    doubled.push(last);
                }
                doubled
            } else {
                track.raw_beat_times.clone()
            }
        }
    };

    // 2. Align downbeat phase with raw_downbeat_times + user's `downbeat_phase_shift`
    // Fix 2: In Dynamic mode, continuously re-sync Bar 1 onto locally detected neural downbeats
    // after breakdowns, pauses, or missed beats so claps & downbeats stay aligned!
    let base_phase = best_downbeat_phase(&beat_times, &track.raw_downbeat_times, bpb);
    let global_phase =
        ((base_phase as i32 + track.downbeat_phase_shift).rem_euclid(bpb as i32)) as usize;

    let mut marks = Vec::with_capacity(beat_times.len() + track.raw_drops.len());
    let mut next_id = 1u64;

    if track.grid_mode == GridMode::Dynamic && !track.raw_downbeat_times.is_empty() {
        let mut bar_num: u32 = 0;
        let mut beat_in_bar: usize = 0;
        let db_match_tol = (period * 0.22).clamp(0.06, 0.14);

        // Precompute which beat indices coincide directly with a neural downbeat
        let is_raw_db: Vec<bool> = beat_times
            .iter()
            .map(|&t| {
                track
                    .raw_downbeat_times
                    .iter()
                    .any(|&db| (db - t).abs() <= db_match_tol)
            })
            .collect();

        // Apply user's phase shift modulo bpb relative to each contiguous post-breakdown segment
        let shift = track.downbeat_phase_shift.rem_euclid(bpb as i32) as usize;
        let mut segment_start = 0usize;
        for (idx, &t) in beat_times.iter().enumerate() {
            let gap_from_prev = if idx > 0 { t - beat_times[idx - 1] } else { 0.0 };
            let after_breakdown = idx > 0 && gap_from_prev > period * 1.55;
            if after_breakdown {
                segment_start = idx;
            }

            // Locally confirmed downbeat inside the current contiguous segment
            let raw_db_here = if shift == 0 {
                is_raw_db[idx]
            } else {
                idx >= segment_start + shift && is_raw_db[idx - shift]
            };

            let start_new_bar = if idx == 0 {
                let pos0 = (bpb - global_phase) % bpb;
                beat_in_bar = pos0;
                bar_num = 1;
                beat_in_bar == 0
            } else if after_breakdown && is_raw_db[idx] {
                // Re-sync cleanly right after a breakdown/pause onto real Bar 1 (+ user phase shift)
                bar_num += 1;
                beat_in_bar = (bpb - shift) % bpb;
                beat_in_bar == 0
            } else if raw_db_here && beat_in_bar >= 1 && beat_in_bar + 1 >= bpb.saturating_sub(1) {
                bar_num += 1;
                beat_in_bar = 0;
                true
            } else {
                beat_in_bar += 1;
                if beat_in_bar >= bpb {
                    beat_in_bar = 0;
                    bar_num += 1;
                    true
                } else {
                    false
                }
            };

            let beat_1based = (beat_in_bar + 1) as u8;
            let kind = if start_new_bar || beat_1based == 1 {
                MarkKind::Downbeat
            } else if track.active_time_sig.is_strong_beat(beat_1based) {
                MarkKind::StrongBeat
            } else {
                MarkKind::RegularBeat
            };

            let energy = sample_waveform_energy(track, t);
            marks.push(BeatMark {
                id: next_id,
                time_sec: t,
                kind,
                bar: bar_num.max(1),
                beat_in_bar: beat_1based,
                energy,
                confidence: 0.9,
                custom_label: None,
            });
            next_id += 1;
        }
    } else {
        let phase_offset = (bpb - global_phase) % bpb;
        for (idx, &t) in beat_times.iter().enumerate() {
            let pos_in_bar = (idx + phase_offset) % bpb;
            let beat_in_bar = (pos_in_bar + 1) as u8;
            let bar = ((idx + phase_offset) / bpb) as u32 + 1;
            let kind = if beat_in_bar == 1 {
                MarkKind::Downbeat
            } else if track.active_time_sig.is_strong_beat(beat_in_bar) {
                MarkKind::StrongBeat
            } else {
                MarkKind::RegularBeat
            };

            let energy = sample_waveform_energy(track, t);
            marks.push(BeatMark {
                id: next_id,
                time_sec: t,
                kind,
                bar,
                beat_in_bar,
                energy,
                confidence: 0.9,
                custom_label: None,
            });
            next_id += 1;
        }
    }

    // 3. Add Energy Drop marks filtered by `drop_sensitivity` (0.0 = strict, 1.0 = sensitive)
    let drop_threshold = 0.85 - (track.drop_sensitivity.clamp(0.0, 1.0) * 0.60);
    for &(drop_t, strength) in &track.raw_drops {
        if strength < drop_threshold {
            continue;
        }
        // Snap drop onto nearest downbeat if within 0.30s so drops lock cleanly to bar starts!
        let mut snapped_t = drop_t;
        let mut bar = 1u32;
        let mut beat_in_bar = 1u8;
        let mut matched_db = false;
        if let Some(nearest) = marks
            .iter()
            .filter(|m| m.kind == MarkKind::Downbeat)
            .min_by(|a, b| {
                (a.time_sec - drop_t)
                    .abs()
                    .partial_cmp(&(b.time_sec - drop_t).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            if (nearest.time_sec - drop_t).abs() <= 0.30 {
                snapped_t = nearest.time_sec;
                bar = nearest.bar;
                beat_in_bar = nearest.beat_in_bar;
                matched_db = true;
            }
        }
        if !matched_db {
            if let Some(any_beat) = marks
                .iter()
                .filter(|m| m.kind != MarkKind::EnergyDrop)
                .min_by(|a, b| {
                    (a.time_sec - drop_t)
                        .abs()
                        .partial_cmp(&(b.time_sec - drop_t).abs())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            {
                bar = any_beat.bar;
                beat_in_bar = any_beat.beat_in_bar;
            }
        }

        marks.push(BeatMark {
            id: next_id,
            time_sec: snapped_t,
            kind: MarkKind::EnergyDrop,
            bar,
            beat_in_bar,
            energy: strength,
            confidence: strength,
            custom_label: None,
        });
        next_id += 1;
    }

    sort_marks_chronologically(&mut marks);

    track.marks = marks;
    track.next_mark_id = next_id;
}

pub fn toggle_bpm_multiplier(track: &mut TrackAnalysis, factor: f64) -> bool {
    let next = track.bpm_multiplier * factor;
    let candidate_bpm = track.detected_bpm * next;
    if (35.0..=260.0).contains(&candidate_bpm) {
        track.bpm_multiplier = next;
        rebuild_marks_from_raw(track);
        true
    } else {
        false
    }
}

pub fn shift_downbeat_phase(track: &mut TrackAnalysis, delta: i32) {
    let bpb = track.active_time_sig.beats_per_bar().max(1) as i32;
    track.downbeat_phase_shift = (track.downbeat_phase_shift + delta).rem_euclid(bpb);
    rebuild_marks_from_raw(track);
}

/// Regenerate only `EnergyDrop` marks from `raw_drops` using `drop_sensitivity`,
/// preserving all manual beat edits!
pub fn rebuild_energy_drops_only(track: &mut TrackAnalysis) {
    track.marks.retain(|m| m.kind != MarkKind::EnergyDrop);
    let drop_threshold = 0.85 - (track.drop_sensitivity.clamp(0.0, 1.0) * 0.60);

    let beats_snapshot: Vec<(f64, MarkKind, u32, u8)> = track
        .marks
        .iter()
        .map(|m| (m.time_sec, m.kind, m.bar, m.beat_in_bar))
        .collect();

    for &(drop_t, strength) in &track.raw_drops {
        if strength < drop_threshold {
            continue;
        }
        let mut snapped_t = drop_t;
        let mut bar = 1u32;
        let mut beat_in_bar = 1u8;
        let mut matched_db = false;
        if let Some(&(db_t, _, db_bar, db_beat)) = beats_snapshot
            .iter()
            .filter(|m| m.1 == MarkKind::Downbeat)
            .min_by(|a, b| {
                (a.0 - drop_t)
                    .abs()
                    .partial_cmp(&(b.0 - drop_t).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        {
            if (db_t - drop_t).abs() <= 0.30 {
                snapped_t = db_t;
                bar = db_bar;
                beat_in_bar = db_beat;
                matched_db = true;
            }
        }
        if !matched_db {
            if let Some(&(_, _, any_bar, any_beat)) = beats_snapshot.iter().min_by(|a, b| {
                (a.0 - drop_t)
                    .abs()
                    .partial_cmp(&(b.0 - drop_t).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                bar = any_bar;
                beat_in_bar = any_beat;
            }
        }
        let id = track.next_mark_id;
        track.next_mark_id += 1;
        track.marks.push(BeatMark {
            id,
            time_sec: snapped_t,
            kind: MarkKind::EnergyDrop,
            bar,
            beat_in_bar,
            energy: strength,
            confidence: strength,
            custom_label: None,
        });
    }

    sort_marks_chronologically(&mut track.marks);
}

pub fn set_time_signature(track: &mut TrackAnalysis, ts: TimeSignature) {
    track.active_time_sig = ts;
    rebuild_marks_from_raw(track);
}

pub fn set_grid_mode(track: &mut TrackAnalysis, mode: GridMode) {
    track.grid_mode = mode;
    rebuild_marks_from_raw(track);
}

pub fn nudge_all_marks(track: &mut TrackAnalysis, delta_sec: f64) {
    for t in &mut track.raw_beat_times {
        *t = (*t + delta_sec).clamp(0.0, track.duration_sec);
    }
    for t in &mut track.raw_downbeat_times {
        *t = (*t + delta_sec).clamp(0.0, track.duration_sec);
    }
    for m in &mut track.marks {
        m.time_sec = (m.time_sec + delta_sec).clamp(0.0, track.duration_sec);
    }
}

pub fn apply_tap_tempo(track: &mut TrackAnalysis, tapped_bpm: f64, anchor_time_sec: f64) {
    if !(40.0..=240.0).contains(&tapped_bpm) {
        return;
    }
    track.detected_bpm = (tapped_bpm * 10.0).round() / 10.0;
    track.bpm_multiplier = 1.0;
    track.grid_mode = GridMode::LockedConstantBpm;
    track.extrapolate_silence = true;
    track.raw_downbeat_times = vec![anchor_time_sec.clamp(0.0, track.duration_sec)];
    track.raw_beat_times = vec![anchor_time_sec.clamp(0.0, track.duration_sec)];
    track.downbeat_phase_shift = 0;
    rebuild_marks_from_raw(track);
}

/// Fix 3: "Re-Lock Grid From Here" (Warp Anchor).
/// Keeps every mark strictly before `anchor_time_sec` untouched, and rebuilds a clean,
/// drift-free beat grid from `anchor_time_sec` to the end of the track using `active_bpm`.
/// `anchor_beat_in_bar` is 1-based (e.g. `1` = Downbeat/Beat 1, `2` = Clap/Backbeat Beat 2).
pub fn relock_grid_from_time(
    track: &mut TrackAnalysis,
    anchor_time_sec: f64,
    anchor_beat_in_bar: u8,
) {
    let anchor = anchor_time_sec.clamp(0.0, track.duration_sec);
    let bpb = track.active_time_sig.beats_per_bar().max(1);
    let start_beat = anchor_beat_in_bar.clamp(1, bpb);
    let period = 60.0 / track.active_bpm.clamp(35.0, 260.0);

    // Keep all earlier marks untouched + preserve EnergyDrops after anchor
    track.marks.retain(|m| {
        m.time_sec < anchor - 0.02 || m.kind == MarkKind::EnergyDrop
    });

    let last_pre_bar = track
        .marks
        .iter()
        .filter(|m| m.time_sec < anchor - 0.02 && m.kind != MarkKind::EnergyDrop)
        .map(|m| m.bar)
        .max()
        .unwrap_or(0);

    // If anchoring on Beat 1, start the next bar; if anchoring mid-bar (e.g. Beat 2 Clap),
    // stay in the current bar (or bar 1 if no earlier beats exist) so bar numbers stay logical.
    let mut cur_bar = if start_beat == 1 || last_pre_bar == 0 {
        last_pre_bar + 1
    } else {
        last_pre_bar
    };

    // Generate clean locked beat grid starting exactly on `anchor` with accurate bar & beat numbers
    let mut k = 0usize;
    loop {
        let t = anchor + (k as f64) * period;
        if t > track.duration_sec + 1e-4 {
            break;
        }
        let beat_in_bar = (((start_beat as usize - 1 + k) % bpb as usize) + 1) as u8;
        if k > 0 && beat_in_bar == 1 {
            cur_bar += 1;
        }
        let kind = if beat_in_bar == 1 {
            MarkKind::Downbeat
        } else if track.active_time_sig.is_strong_beat(beat_in_bar) {
            MarkKind::StrongBeat
        } else {
            MarkKind::RegularBeat
        };
        let id = track.next_mark_id;
        track.next_mark_id += 1;
        let energy = sample_waveform_energy(track, t);
        track.marks.push(BeatMark {
            id,
            time_sec: t.min(track.duration_sec),
            kind,
            bar: cur_bar,
            beat_in_bar,
            energy,
            confidence: 1.0,
            custom_label: None,
        });
        k += 1;
    }

    sort_marks_chronologically(&mut track.marks);
    sync_energy_drop_bar_numbers(&mut track.marks);
}

/// Insert a manual mark at `time_sec` and return its new ID.
pub fn add_mark(track: &mut TrackAnalysis, time_sec: f64, kind: MarkKind) -> u64 {
    let t = time_sec.clamp(0.0, track.duration_sec);
    let id = track.next_mark_id;
    track.next_mark_id += 1;
    let energy = sample_waveform_energy(track, t);
    track.marks.push(BeatMark {
        id,
        time_sec: t,
        kind,
        bar: 1,
        beat_in_bar: 1,
        energy,
        confidence: 1.0,
        custom_label: None,
    });
    sort_and_renumber_marks(track);
    id
}

pub fn move_mark(track: &mut TrackAnalysis, id: u64, new_time_sec: f64) {
    let t = new_time_sec.clamp(0.0, track.duration_sec);
    let energy = sample_waveform_energy(track, t);
    if let Some(m) = track.marks.iter_mut().find(|m| m.id == id) {
        m.time_sec = t;
        m.energy = energy;
    }
    sort_and_renumber_marks(track);
}

/// Move multiple marks in one batch and renumber once (O(N + K) for smooth live group dragging).
pub fn move_marks(track: &mut TrackAnalysis, updates: &[(u64, f64)]) {
    if updates.is_empty() {
        return;
    }
    let dur = track.duration_sec;
    let by_id: std::collections::HashMap<u64, (f64, f32)> = updates
        .iter()
        .map(|&(id, raw_t)| {
            let t = raw_t.clamp(0.0, dur);
            (id, (t, sample_waveform_energy(track, t)))
        })
        .collect();
    for m in &mut track.marks {
        if let Some(&(t, energy)) = by_id.get(&m.id) {
            m.time_sec = t;
            m.energy = energy;
        }
    }
    sort_and_renumber_marks(track);
}

pub fn delete_mark(track: &mut TrackAnalysis, id: u64) -> bool {
    let before = track.marks.len();
    track.marks.retain(|m| m.id != id);
    if track.marks.len() != before {
        sort_and_renumber_marks(track);
        true
    } else {
        false
    }
}

/// Delete multiple marks by ID in one batch and renumber once. Returns number removed.
pub fn delete_marks(track: &mut TrackAnalysis, ids: &std::collections::HashSet<u64>) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let before = track.marks.len();
    track.marks.retain(|m| !ids.contains(&m.id));
    let removed = before.saturating_sub(track.marks.len());
    if removed > 0 {
        sort_and_renumber_marks(track);
    }
    removed
}

pub fn reclassify_mark(track: &mut TrackAnalysis, id: u64, new_kind: MarkKind) {
    if let Some(m) = track.marks.iter_mut().find(|m| m.id == id) {
        m.kind = new_kind;
    }
    sort_and_renumber_marks(track);
}

/// Reclassify multiple marks by ID in one batch and renumber once. Returns number updated.
pub fn reclassify_marks(
    track: &mut TrackAnalysis,
    ids: &std::collections::HashSet<u64>,
    new_kind: MarkKind,
) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let mut changed = 0usize;
    for m in &mut track.marks {
        if ids.contains(&m.id) {
            m.kind = new_kind;
            changed += 1;
        }
    }
    if changed > 0 {
        sort_and_renumber_marks(track);
    }
    changed
}

pub fn set_mark_label(track: &mut TrackAnalysis, id: u64, label: Option<String>) {
    if let Some(m) = track.marks.iter_mut().find(|m| m.id == id) {
        m.custom_label = label;
    }
}

fn sort_marks_chronologically(marks: &mut [BeatMark]) {
    marks.sort_by(|a, b| {
        a.time_sec
            .partial_cmp(&b.time_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                // Place beat marks before EnergyDrops at identical timestamps so drops inherit the co-located beat's bar/beat
                let a_drop = a.kind == MarkKind::EnergyDrop;
                let b_drop = b.kind == MarkKind::EnergyDrop;
                a_drop
                    .cmp(&b_drop)
                    .then_with(|| b.kind.priority().cmp(&a.kind.priority()))
            })
    });
}

fn sync_energy_drop_bar_numbers(marks: &mut [BeatMark]) {
    let mut cur_bar = 1u32;
    let mut cur_beat = 1u8;
    for m in marks.iter_mut() {
        if m.kind == MarkKind::EnergyDrop {
            m.bar = cur_bar;
            m.beat_in_bar = cur_beat;
        } else {
            cur_bar = m.bar.max(1);
            cur_beat = m.beat_in_bar.max(1);
        }
    }
}

/// Keep marks sorted chronologically and update Bar / Beat numbers after direct edits.
pub fn sort_and_renumber_marks(track: &mut TrackAnalysis) {
    sort_marks_chronologically(&mut track.marks);

    let bpb = track.active_time_sig.beats_per_bar();
    let mut current_bar = 0u32;
    let mut current_beat = 0u8;

    for m in &mut track.marks {
        match m.kind {
            MarkKind::Downbeat => {
                current_bar += 1;
                current_beat = 1;
                m.bar = current_bar;
                m.beat_in_bar = 1;
            }
            MarkKind::StrongBeat | MarkKind::RegularBeat => {
                if current_bar == 0 {
                    current_bar = 1;
                }
                current_beat = if current_beat >= bpb {
                    current_bar += 1;
                    1
                } else {
                    current_beat + 1
                };
                m.bar = current_bar;
                m.beat_in_bar = current_beat;
            }
            MarkKind::EnergyDrop => {
                m.bar = current_bar.max(1);
                m.beat_in_bar = current_beat.max(1);
            }
        }
    }
}

fn find_best_grid_anchor(beats: &[f64], downbeats: &[f64], period: f64) -> f64 {
    if beats.is_empty() {
        return 0.0;
    }
    // Circular mean phase of all detected beats modulo `period` for sub-millisecond grid phase lock!
    let two_pi = std::f64::consts::TAU;
    let mut sin_sum = 0.0;
    let mut cos_sum = 0.0;
    for &t in beats {
        let angle = two_pi * (t.rem_euclid(period)) / period;
        sin_sum += angle.sin();
        cos_sum += angle.cos();
    }
    let phase_angle = sin_sum.atan2(cos_sum).rem_euclid(two_pi);
    let base_phase_sec = phase_angle * period / two_pi;

    // Anchor onto the first detected downbeat snapped to that exact circular phase
    if let Some(&first_db) = downbeats.first() {
        let k = ((first_db - base_phase_sec) / period).round();
        return (base_phase_sec + k * period).max(0.0);
    }
    base_phase_sec
}

fn best_downbeat_phase(beats: &[f64], downbeats: &[f64], bpb: usize) -> usize {
    if beats.is_empty() || downbeats.is_empty() || bpb == 0 {
        return 0;
    }
    let mut votes = vec![0usize; bpb];
    for &db in downbeats {
        if let Some((idx, _)) = beats.iter().enumerate().min_by(|(_, a), (_, b)| {
            (*a - db)
                .abs()
                .partial_cmp(&(*b - db).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            if (beats[idx] - db).abs() < 0.18 {
                votes[idx % bpb] += 1;
            }
        }
    }
    votes
        .iter()
        .enumerate()
        .max_by_key(|&(_, &v)| v)
        .map(|(p, _)| p)
        .unwrap_or(0)
}

fn sample_waveform_energy(track: &TrackAnalysis, time_sec: f64) -> f32 {
    if track.waveform.is_empty() {
        return 0.5;
    }
    let idx = ((time_sec * track.waveform_bins_per_sec as f64).round() as usize)
        .min(track.waveform.len() - 1);
    track.waveform[idx].peak
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sample_track() -> TrackAnalysis {
        TrackAnalysis {
            title: "Test".into(),
            file_path: String::new(),
            sample_rate: 44100,
            channels: 2,
            duration_sec: 10.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 1.0,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::LockedConstantBpm,
            detection_engine: "test".into(),
            downbeat_phase_shift: 0,
            extrapolate_silence: false,
            in_point_sec: 0.0,
            out_point_sec: 10.0,
            marks: vec![
                BeatMark {
                    id: 1,
                    time_sec: 1.0,
                    kind: MarkKind::Downbeat,
                    bar: 1,
                    beat_in_bar: 1,
                    energy: 0.8,
                    confidence: 1.0,
                    custom_label: None,
                },
                BeatMark {
                    id: 2,
                    time_sec: 1.5,
                    kind: MarkKind::RegularBeat,
                    bar: 1,
                    beat_in_bar: 2,
                    energy: 0.5,
                    confidence: 1.0,
                    custom_label: None,
                },
                BeatMark {
                    id: 3,
                    time_sec: 3.0,
                    kind: MarkKind::Downbeat,
                    bar: 2,
                    beat_in_bar: 1,
                    energy: 0.9,
                    confidence: 1.0,
                    custom_label: None,
                },
                BeatMark {
                    id: 4,
                    time_sec: 5.0,
                    kind: MarkKind::Downbeat,
                    bar: 3,
                    beat_in_bar: 1,
                    energy: 0.9,
                    confidence: 1.0,
                    custom_label: None,
                },
            ],
            raw_beat_times: vec![],
            raw_downbeat_times: vec![],
            raw_drops: vec![],
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 100.0,
            waveform: vec![],
            next_mark_id: 5,
        }
    }

    #[test]
    fn batch_move_reclassify_and_delete_preserve_other_marks() {
        let mut track = sample_track();
        let selected: HashSet<u64> = [1, 3].into_iter().collect();

        // Batch move selected downbeats +0.2s while leaving id=2 and id=4 untouched
        move_marks(&mut track, &[(1, 1.2), (3, 3.2)]);
        assert!((track.marks.iter().find(|m| m.id == 1).unwrap().time_sec - 1.2).abs() < 1e-9);
        assert!((track.marks.iter().find(|m| m.id == 2).unwrap().time_sec - 1.5).abs() < 1e-9);
        assert!((track.marks.iter().find(|m| m.id == 3).unwrap().time_sec - 3.2).abs() < 1e-9);
        assert!((track.marks.iter().find(|m| m.id == 4).unwrap().time_sec - 5.0).abs() < 1e-9);

        // Batch reclassify selected marks to EnergyDrop
        let changed = reclassify_marks(&mut track, &selected, MarkKind::EnergyDrop);
        assert_eq!(changed, 2);
        assert_eq!(track.marks.iter().find(|m| m.id == 1).unwrap().kind, MarkKind::EnergyDrop);
        assert_eq!(track.marks.iter().find(|m| m.id == 3).unwrap().kind, MarkKind::EnergyDrop);
        assert_eq!(track.marks.iter().find(|m| m.id == 2).unwrap().kind, MarkKind::RegularBeat);

        // Batch delete selected marks
        let removed = delete_marks(&mut track, &selected);
        assert_eq!(removed, 2);
        let remaining_ids: Vec<u64> = track.marks.iter().map(|m| m.id).collect();
        assert_eq!(remaining_ids, vec![2, 4]);
    }
}

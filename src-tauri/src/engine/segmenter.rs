use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentRange {
    pub idx: u32,
    pub start_byte: u64,
    pub end_byte: u64,
    pub written: u64,
    pub completed: bool,
    pub assigned_worker: Option<usize>,
}

impl SegmentRange {
    pub fn remaining(&self) -> u64 {
        let current = self.start_byte + self.written;
        if current > self.end_byte {
            0
        } else {
            self.end_byte - current + 1
        }
    }

    pub fn is_done(&self) -> bool {
        self.completed || self.remaining() == 0
    }
}

pub fn split_into_segments(total_size: u64, num_segments: u32, min_segment_size: u64) -> Vec<SegmentRange> {
    if total_size == 0 || num_segments <= 1 {
        return vec![SegmentRange {
            idx: 0,
            start_byte: 0,
            end_byte: total_size.saturating_sub(1),
            written: 0,
            completed: false,
            assigned_worker: None,
        }];
    }

    let actual_segments = (total_size / min_segment_size.max(1))
        .max(1)
        .min(num_segments as u64) as u32;

    if actual_segments <= 1 {
        return vec![SegmentRange {
            idx: 0,
            start_byte: 0,
            end_byte: total_size - 1,
            written: 0,
            completed: false,
            assigned_worker: None,
        }];
    }

    let chunk_size = total_size / actual_segments as u64;
    let mut segments = Vec::with_capacity(actual_segments as usize);

    for i in 0..actual_segments {
        let start = i as u64 * chunk_size;
        let end = if i == actual_segments - 1 {
            total_size - 1
        } else {
            (i as u64 + 1) * chunk_size - 1
        };

        segments.push(SegmentRange {
            idx: i,
            start_byte: start,
            end_byte: end,
            written: 0,
            completed: false,
            assigned_worker: None,
        });
    }

    segments
}

#[derive(Debug, Clone)]
pub struct SegmentManager {
    pub total_size: u64,
    pub min_segment_size: u64,
    pub segments: Vec<SegmentRange>,
    next_idx: u32,
}

impl SegmentManager {
    pub fn new(total_size: u64, initial_segments: u32, min_segment_size: u64) -> Self {
        let initial = split_into_segments(total_size, initial_segments, min_segment_size);
        let count = initial.len() as u32;
        Self {
            total_size,
            min_segment_size,
            segments: initial,
            next_idx: count,
        }
    }

    pub fn from_saved_segments(
        total_size: u64,
        min_segment_size: u64,
        records: Vec<crate::storage::db::SegmentRecord>,
    ) -> Self {
        let max_idx = records.iter().map(|r| r.idx).max().unwrap_or(0);
        let segments = records
            .into_iter()
            .map(|r| SegmentRange {
                idx: r.idx,
                start_byte: r.start_byte,
                end_byte: r.end_byte,
                written: r.written,
                completed: r.written >= (r.end_byte.saturating_sub(r.start_byte) + 1),
                assigned_worker: None,
            })
            .collect();

        Self {
            total_size,
            min_segment_size,
            segments,
            next_idx: max_idx + 1,
        }
    }

    pub fn claim_next_segment(&mut self, worker_id: usize) -> Option<SegmentRange> {
        for seg in &mut self.segments {
            if !seg.completed && seg.assigned_worker.is_none() && seg.remaining() > 0 {
                seg.assigned_worker = Some(worker_id);
                return Some(seg.clone());
            }
        }
        None
    }

    pub fn record_progress(&mut self, idx: u32, bytes_written: u64) {
        if let Some(seg) = self.segments.iter_mut().find(|s| s.idx == idx) {
            seg.written += bytes_written;
            if seg.remaining() == 0 {
                seg.completed = true;
                seg.assigned_worker = None;
            }
        }
    }

    pub fn mark_completed(&mut self, idx: u32) {
        if let Some(seg) = self.segments.iter_mut().find(|s| s.idx == idx) {
            seg.completed = true;
            seg.assigned_worker = None;
        }
    }

    pub fn get_segment(&self, idx: u32) -> Option<SegmentRange> {
        self.segments.iter().find(|s| s.idx == idx).cloned()
    }

    pub fn try_dynamic_split(&mut self, worker_id: usize, min_segment_size: u64) -> Option<SegmentRange> {
        // Find segment with largest remaining bytes
        let mut best_idx = None;
        let mut max_remaining = 0u64;

        for seg in &self.segments {
            if !seg.completed {
                let rem = seg.remaining();
                if rem > max_remaining {
                    max_remaining = rem;
                    best_idx = Some(seg.idx);
                }
            }
        }

        let target_idx = best_idx?;
        if max_remaining < 2 * min_segment_size {
            return None;
        }

        let new_seg_idx = self.next_idx;
        self.next_idx += 1;

        let seg = self.segments.iter_mut().find(|s| s.idx == target_idx)?;
        let current_cursor = seg.start_byte + seg.written;
        let half = max_remaining / 2;
        let split_at = current_cursor + half;

        let old_end = seg.end_byte;
        seg.end_byte = split_at.saturating_sub(1);

        let new_seg = SegmentRange {
            idx: new_seg_idx,
            start_byte: split_at,
            end_byte: old_end,
            written: 0,
            completed: false,
            assigned_worker: Some(worker_id),
        };

        self.segments.push(new_seg.clone());
        Some(new_seg)
    }

    pub fn is_all_completed(&self) -> bool {
        self.segments.iter().all(|s| s.is_done())
    }

    pub fn total_downloaded(&self) -> u64 {
        self.segments.iter().map(|s| s.written).sum()
    }

    /// Invariant check: segments must be sorted, non-overlapping, contiguous from 0 to total_size-1
    pub fn verify_invariants(&self) -> Result<(), String> {
        if self.segments.is_empty() {
            if self.total_size == 0 {
                return Ok(());
            }
            return Err("Empty segments for non-zero total_size".into());
        }

        let mut sorted = self.segments.clone();
        sorted.sort_by_key(|s| s.start_byte);

        if sorted[0].start_byte != 0 {
            return Err(format!(
                "First segment starts at {}, expected 0",
                sorted[0].start_byte
            ));
        }

        for i in 0..sorted.len() - 1 {
            let curr = &sorted[i];
            let next = &sorted[i + 1];

            if curr.end_byte >= next.start_byte {
                return Err(format!(
                    "Overlap detected between segment {} [{}..{}] and segment {} [{}..{}]",
                    curr.idx, curr.start_byte, curr.end_byte, next.idx, next.start_byte, next.end_byte
                ));
            }

            if curr.end_byte + 1 != next.start_byte {
                return Err(format!(
                    "Gap detected between segment {} [{}..{}] and segment {} [{}..{}]",
                    curr.idx, curr.start_byte, curr.end_byte, next.idx, next.start_byte, next.end_byte
                ));
            }
        }

        let last = sorted.last().unwrap();
        if last.end_byte != self.total_size - 1 {
            return Err(format!(
                "Last segment ends at {}, expected {}",
                last.end_byte,
                self.total_size - 1
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_split_no_gap_or_overlap() {
        let total = 10_000_000; // 10 MB
        let segments = split_into_segments(total, 8, 512 * 1024);

        assert_eq!(segments.len(), 8);
        assert_eq!(segments[0].start_byte, 0);
        assert_eq!(segments.last().unwrap().end_byte, total - 1);

        for i in 0..segments.len() - 1 {
            assert_eq!(segments[i].end_byte + 1, segments[i + 1].start_byte);
        }
    }

    #[test]
    fn test_dynamic_segmentation_property() {
        let total = 100_000_000; // 100 MB
        let mut mgr = SegmentManager::new(total, 4, 512 * 1024);
        mgr.verify_invariants().expect("Initial segments must have valid invariants");

        // Simulate worker 0 downloading 10 MB on segment 0
        mgr.claim_next_segment(0);
        mgr.record_progress(0, 10_000_000);
        mgr.verify_invariants().expect("Invariants must hold after progress");

        // Worker 4 comes along and splits the largest remaining segment
        let split_seg = mgr.try_dynamic_split(4, 512 * 1024);
        assert!(split_seg.is_some());
        mgr.verify_invariants().expect("Invariants must hold after 1st dynamic split");

        // Split repeatedly 50 times at arbitrary stages
        for w in 5..55 {
            let _ = mgr.try_dynamic_split(w, 512 * 1024);
            mgr.verify_invariants().expect("Invariants must hold after iterative dynamic split");
        }
    }
}

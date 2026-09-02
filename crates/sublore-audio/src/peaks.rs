//! Folding samples into millisecond buckets, as they arrive. See BACKLOG.md M2.4.
//!
//! One bucket per millisecond, holding the smallest and the largest sample in it. Decision 11
//! makes the millisecond the product's finest unit, so nothing ever needs to draw between two
//! buckets and there is no second resolution level to keep in step with this one.

/// The rate every extraction asks ffmpeg for. A multiple of a thousand, so a bucket is a whole
/// number of samples and no bucket boundary ever falls inside one.
pub const SAMPLE_RATE: u32 = 48_000;
/// Samples in one bucket.
pub const SAMPLES_PER_BUCKET: usize = (SAMPLE_RATE / 1000) as usize;
/// How many buckets are handed over at a time: one second of media, four kilobytes.
pub const CHUNK_BUCKETS: usize = 1000;

/// One millisecond of audio: the extremes of it, which is what a waveform draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bucket {
    pub min: i16,
    pub max: i16,
}

/// The fold: bytes of little-endian `s16le` in, buckets out in chunks.
///
/// It holds at most one chunk plus one bucket, so peaking a film costs the same memory as peaking
/// a trailer. The caller receives each chunk with the millisecond its first bucket starts at.
#[derive(Debug, Default)]
pub struct Peaks {
    ready: Vec<Bucket>,
    /// Buckets already handed to the callback, which is also the millisecond `ready[0]` starts at.
    emitted: u32,
    min: i16,
    max: i16,
    filled: usize,
    /// The low half of a sample split across two reads of the pipe.
    carry: Option<u8>,
}

impl Peaks {
    pub fn new() -> Self {
        Self {
            ready: Vec::with_capacity(CHUNK_BUCKETS),
            ..Self::default()
        }
    }

    /// Fold `bytes` in, handing `emit` every chunk that completes.
    pub fn push(&mut self, bytes: &[u8], emit: &mut dyn FnMut(u32, &[Bucket])) {
        let mut rest = bytes;
        if let Some(low) = self.carry.take() {
            let Some((high, tail)) = rest.split_first() else {
                self.carry = Some(low);
                return;
            };
            self.sample(i16::from_le_bytes([low, *high]), emit);
            rest = tail;
        }
        let mut pairs = rest.chunks_exact(2);
        for pair in &mut pairs {
            self.sample(i16::from_le_bytes([pair[0], pair[1]]), emit);
        }
        self.carry = pairs.remainder().first().copied();
    }

    /// Close the last bucket, hand over what is left, and report how many buckets there were.
    ///
    /// A trailing half sample is dropped: a stream that ends mid-sample has one byte of noise in
    /// it, not a millisecond.
    pub fn finish(mut self, emit: &mut dyn FnMut(u32, &[Bucket])) -> u32 {
        if self.filled > 0 {
            // The last bucket of a file is short unless the media is a whole number of
            // milliseconds long. It is still a millisecond of the timeline.
            self.ready.push(Bucket {
                min: self.min,
                max: self.max,
            });
            self.filled = 0;
        }
        self.flush(emit);
        self.emitted
    }

    fn sample(&mut self, value: i16, emit: &mut dyn FnMut(u32, &[Bucket])) {
        if self.filled == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.filled += 1;
        if self.filled == SAMPLES_PER_BUCKET {
            self.ready.push(Bucket {
                min: self.min,
                max: self.max,
            });
            self.filled = 0;
            if self.ready.len() >= CHUNK_BUCKETS {
                self.flush(emit);
            }
        }
    }

    fn flush(&mut self, emit: &mut dyn FnMut(u32, &[Bucket])) {
        if self.ready.is_empty() {
            return;
        }
        emit(self.emitted, &self.ready);
        // Saturating rather than wrapping: 4.29 billion buckets is 49 days of media, and a wrapped
        // count would be handed out as a position.
        self.emitted = self.emitted.saturating_add(self.ready.len() as u32);
        self.ready.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{Bucket, Peaks, CHUNK_BUCKETS, SAMPLES_PER_BUCKET};

    /// Collects what the fold hands over, in order, with the millisecond each chunk started at.
    #[derive(Default)]
    struct Sink {
        chunks: Vec<(u32, Vec<Bucket>)>,
    }

    impl Sink {
        fn buckets(&self) -> Vec<Bucket> {
            self.chunks
                .iter()
                .flat_map(|(_, buckets)| buckets.iter().copied())
                .collect()
        }
    }

    fn fold(bytes: &[&[u8]]) -> (Sink, u32) {
        let mut sink = Sink::default();
        let mut peaks = Peaks::new();
        // The closure borrows the sink, so it ends before the sink is handed back.
        let total = {
            let mut emit = |first: u32, buckets: &[Bucket]| {
                sink.chunks.push((first, buckets.to_vec()));
            };
            for slice in bytes {
                peaks.push(slice, &mut emit);
            }
            peaks.finish(&mut emit)
        };
        (sink, total)
    }

    fn samples(values: &[i16]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn a_millisecond_of_samples_is_one_bucket_holding_its_extremes() {
        let mut values = vec![0i16; SAMPLES_PER_BUCKET];
        values[7] = -21_000;
        values[19] = 4_000;
        let bytes = samples(&values);
        let (sink, total) = fold(&[&bytes]);
        assert_eq!(total, 1);
        assert_eq!(
            sink.buckets(),
            vec![Bucket {
                min: -21_000,
                max: 4_000
            }]
        );
        assert_eq!(
            sink.chunks[0].0, 0,
            "the first chunk starts at millisecond 0"
        );
    }

    #[test]
    fn full_scale_is_carried_both_ways_without_overflowing() {
        let mut values = vec![0i16; SAMPLES_PER_BUCKET];
        values[0] = i16::MIN;
        values[1] = i16::MAX;
        let (sink, _) = fold(&[&samples(&values)]);
        assert_eq!(
            sink.buckets(),
            vec![Bucket {
                min: i16::MIN,
                max: i16::MAX
            }]
        );
    }

    #[test]
    fn a_sample_split_across_two_reads_is_still_one_sample() {
        let values = vec![i16::MAX; SAMPLES_PER_BUCKET];
        let bytes = samples(&values);
        // Split inside the third sample, where a pipe read is free to land.
        let (head, tail) = bytes.split_at(5);
        let (sink, total) = fold(&[head, tail]);
        assert_eq!(total, 1);
        assert_eq!(
            sink.buckets(),
            vec![Bucket {
                min: i16::MAX,
                max: i16::MAX
            }],
            "a carried byte must not be read as a sample of its own"
        );
    }

    #[test]
    fn a_byte_at_a_time_folds_to_the_same_buckets_as_one_read() {
        let values: Vec<i16> = (0..SAMPLES_PER_BUCKET * 2)
            .map(|index| (index as i16) * 100 - 1000)
            .collect();
        let bytes = samples(&values);
        let one_read = fold(&[&bytes]);
        let mut singles: Vec<&[u8]> = Vec::new();
        for index in 0..bytes.len() {
            singles.push(&bytes[index..index + 1]);
        }
        let byte_at_a_time = fold(&singles);
        assert_eq!(one_read.1, 2);
        assert_eq!(byte_at_a_time.1, one_read.1);
        assert_eq!(byte_at_a_time.0.buckets(), one_read.0.buckets());
    }

    #[test]
    fn a_short_last_bucket_is_still_a_bucket() {
        let mut values = vec![0i16; SAMPLES_PER_BUCKET + 1];
        values[SAMPLES_PER_BUCKET] = 900;
        let (sink, total) = fold(&[&samples(&values)]);
        assert_eq!(total, 2, "one sample past a whole millisecond is a bucket");
        assert_eq!(sink.buckets()[1], Bucket { min: 900, max: 900 });
    }

    #[test]
    fn a_trailing_half_sample_is_dropped_rather_than_read_as_silence() {
        let mut bytes = samples(&[500i16; SAMPLES_PER_BUCKET]);
        bytes.push(0x7f);
        let (sink, total) = fold(&[&bytes]);
        assert_eq!(total, 1);
        assert_eq!(sink.buckets(), vec![Bucket { min: 500, max: 500 }]);
    }

    #[test]
    fn nothing_in_produces_nothing_out() {
        let (sink, total) = fold(&[]);
        assert_eq!(total, 0);
        assert!(sink.chunks.is_empty(), "an empty stream emits no chunk");
    }

    #[test]
    fn chunks_arrive_a_second_at_a_time_and_each_names_its_own_millisecond() {
        // Two full chunks and one bucket over, so the tail is handed over by finish.
        let count = SAMPLES_PER_BUCKET * (CHUNK_BUCKETS * 2 + 1);
        let (sink, total) = fold(&[&samples(&vec![7i16; count])]);
        assert_eq!(total, (CHUNK_BUCKETS * 2 + 1) as u32);
        assert_eq!(sink.chunks.len(), 3);
        assert_eq!(sink.chunks[0].0, 0);
        assert_eq!(sink.chunks[0].1.len(), CHUNK_BUCKETS);
        assert_eq!(sink.chunks[1].0, CHUNK_BUCKETS as u32);
        assert_eq!(sink.chunks[1].1.len(), CHUNK_BUCKETS);
        assert_eq!(sink.chunks[2].0, (CHUNK_BUCKETS * 2) as u32);
        assert_eq!(sink.chunks[2].1.len(), 1);
    }

    #[test]
    fn chunk_boundaries_do_not_depend_on_how_the_bytes_arrived() {
        let count = SAMPLES_PER_BUCKET * (CHUNK_BUCKETS + 3);
        let values: Vec<i16> = (0..count)
            .map(|index| (index % 3_000) as i16 - 1_500)
            .collect();
        let bytes = samples(&values);
        let one_read = fold(&[&bytes]);
        // A read that lands mid-sample and mid-bucket, the two cases a pipe produces.
        let (head, tail) = bytes.split_at(SAMPLES_PER_BUCKET * 2 * 5 + 1);
        let two_reads = fold(&[head, tail]);
        assert_eq!(two_reads.1, one_read.1);
        assert_eq!(
            two_reads
                .0
                .chunks
                .iter()
                .map(|(first, buckets)| (*first, buckets.len()))
                .collect::<Vec<_>>(),
            one_read
                .0
                .chunks
                .iter()
                .map(|(first, buckets)| (*first, buckets.len()))
                .collect::<Vec<_>>()
        );
        assert_eq!(two_reads.0.buckets(), one_read.0.buckets());
    }
}

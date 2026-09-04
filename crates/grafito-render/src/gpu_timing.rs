//! GPU timestamp query helpers for compute passes.
//!
//! Gated behind the `profiling` feature: with the feature disabled every helper
//! compiles to a no-op and compute passes keep `timestamp_writes: None`, so
//! release builds pay zero cost. When enabled and the adapter exposes
//! [`wgpu::Features::TIMESTAMP_QUERY`], each pipeline records a begin/end
//! timestamp pair per pass and logs the GPU delta in nanoseconds after the
//! synchronous readback poll.
//!
//! Each pipeline owns a [`GpuTimingHandle`] with `2 * pass_count` slots. A
//! single-pass pipeline uses 2 slots; the parametric batch uses `2 * N` slots
//! (one pair per curve dispatch). Query sets are reused across frames because
//! every dispatch is followed by a synchronous bounded poll before the next
//! submit, so the previous frame's queries are always complete.

/// Handle stored in each compute pipeline. With `profiling` disabled this is
/// the unit type so the field costs nothing.
#[cfg(feature = "profiling")]
pub type GpuTimingHandle = Option<GpuTiming>;

/// Handle stored in each compute pipeline. With `profiling` disabled this is
/// the unit type so the field costs nothing.
#[cfg(not(feature = "profiling"))]
pub type GpuTimingHandle = ();

/// Create the timing handle for a pipeline. Returns `None` (or `()` without
/// the feature) when the adapter lacks `TIMESTAMP_QUERY`.
#[cfg(feature = "profiling")]
pub fn create(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    pass_count: u32,
) -> GpuTimingHandle {
    GpuTiming::new(device, queue, label, pass_count)
}

/// Create the timing handle for a pipeline. Returns `None` (or `()` without
/// the feature) when the adapter lacks `TIMESTAMP_QUERY`.
#[cfg(not(feature = "profiling"))]
pub fn create(
    _device: &wgpu::Device,
    _queue: &wgpu::Queue,
    _label: &str,
    _pass_count: u32,
) -> GpuTimingHandle {
}

/// `timestamp_writes` for one pass of a pipeline. `pass_index` selects the
/// 2-slot pair reserved for that pass.
#[cfg(feature = "profiling")]
pub fn timestamp_writes(
    timing: &GpuTimingHandle,
    pass_index: u32,
) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
    timing.as_ref().map(|timing| timing.pass_writes(pass_index))
}

/// `timestamp_writes` for one pass of a pipeline. `pass_index` selects the
/// 2-slot pair reserved for that pass.
#[cfg(not(feature = "profiling"))]
pub fn timestamp_writes(
    _timing: &GpuTimingHandle,
    _pass_index: u32,
) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
    None
}

/// Record the query resolve + copy into the encoder after the timed passes.
#[cfg(feature = "profiling")]
pub fn resolve(timing: &GpuTimingHandle, encoder: &mut wgpu::CommandEncoder) {
    if let Some(timing) = timing {
        timing.resolve(encoder);
    }
}

/// Record the query resolve + copy into the encoder after the timed passes.
#[cfg(not(feature = "profiling"))]
pub fn resolve(_timing: &GpuTimingHandle, _encoder: &mut wgpu::CommandEncoder) {}

/// Read back and log the GPU delta for each timed pass. Call after the
/// synchronous readback poll so the resolve has completed.
#[cfg(feature = "profiling")]
pub fn read_and_log(timing: &GpuTimingHandle, device: &wgpu::Device, label: &str) {
    if let Some(timing) = timing {
        timing.read_and_log(device, label);
    }
}

/// Read back and log the GPU delta for each timed pass. Call after the
/// synchronous readback poll so the resolve has completed.
#[cfg(not(feature = "profiling"))]
pub fn read_and_log(_timing: &GpuTimingHandle, _device: &wgpu::Device, _label: &str) {}

/// GPU timestamp query set plus resolve/readback buffers for one pipeline.
#[cfg(feature = "profiling")]
pub struct GpuTiming {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    timestamp_period: f32,
    pass_count: u32,
}

#[cfg(feature = "profiling")]
impl GpuTiming {
    /// Create a timing set with `2 * pass_count` slots. Returns `None` when the
    /// adapter lacks [`wgpu::Features::TIMESTAMP_QUERY`].
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        label: &str,
        pass_count: u32,
    ) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let slot_count = pass_count.saturating_mul(2).max(2);
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some(&format!("{label} Timing Query Set")),
            ty: wgpu::QueryType::Timestamp,
            count: slot_count,
        });
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label} Timing Resolve")),
            size: (slot_count as u64) * std::mem::size_of::<u64>() as u64,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("{label} Timing Readback")),
            size: (slot_count as u64) * std::mem::size_of::<u64>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let timestamp_period = queue.get_timestamp_period();
        Some(Self {
            query_set,
            resolve_buffer,
            readback_buffer,
            timestamp_period,
            pass_count,
        })
    }

    fn pass_writes(&self, pass_index: u32) -> wgpu::ComputePassTimestampWrites<'_> {
        let base = pass_index.saturating_mul(2);
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(base),
            end_of_pass_write_index: Some(base + 1),
        }
    }

    fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        let slot_count = self.pass_count.saturating_mul(2).max(2);
        encoder.resolve_query_set(&self.query_set, 0..slot_count, &self.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &self.resolve_buffer,
            0,
            &self.readback_buffer,
            0,
            (slot_count as u64) * std::mem::size_of::<u64>() as u64,
        );
    }

    fn read_and_log(&self, device: &wgpu::Device, label: &str) {
        let slice = self.readback_buffer.slice(..);
        let map_ok = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let map_ok_clone = map_ok.clone();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                map_ok_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        // The main readback poll already waited for the GPU work; a short
        // bounded poll is enough for the resolve copy to land.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
        while !map_ok.load(std::sync::atomic::Ordering::SeqCst) {
            if std::time::Instant::now() >= deadline {
                log::warn!("{label}: GPU timing readback timed out");
                return;
            }
            device.poll(wgpu::Maintain::Poll);
            std::thread::yield_now();
        }
        let data = slice.get_mapped_range();
        let ticks: &[u64] = bytemuck::cast_slice(&data);
        for pass in 0..self.pass_count {
            let start = ticks[(pass * 2) as usize];
            let end = ticks[(pass * 2 + 1) as usize];
            let delta_ns = end.saturating_sub(start) as f64 * self.timestamp_period as f64;
            log::debug!("{label} pass {pass}: {delta_ns:.1} ns GPU");
        }
        drop(data);
        self.readback_buffer.unmap();
    }
}

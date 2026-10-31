use super::*;
use solidcraft_geom::{Aabb3, Mesh};

#[test]
fn raster_ev_status_and_task_capture_do_not_change_dynamic_fees_or_enter_registered_work() {
    for rig in [None, Some(Vec::new())] {
        let host = CpuHost::cooperative();
        let mut input = request();
        input.raster_lights = rig;
        let zero = host.admit(&empty(), &input, JobOptions::default()).unwrap();
        input.raster_exposure_ev = 1.0;
        assert_eq!(host.admit(&empty(), &input, JobOptions::default()).unwrap().estimated_total_bytes, zero.estimated_total_bytes);
        let job = host.submit(id(840, 1), 1, empty(), input).unwrap();
        assert_eq!(job.status().unwrap().request.raster_exposure_ev, 1.0);
        drain(&host);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        assert_eq!(job.status().unwrap().request.raster_exposure_ev, 1.0);
    }
    let (host, _) = probe_host(1024);
    let mut input = request();
    input.raster_exposure_ev = 1.0;
    assert_eq!(host.admit(&empty(), &input, JobOptions::default()).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(host.submit(id(841, 1), 1, empty(), input).err().unwrap().code, ErrorCode::InvalidRequest);
    assert_eq!(host.admitted_bytes(), 0);
    assert!(!host.pump(1).unwrap());
}

#[test]
fn raster_rig_charges_original_and_task_capacity_then_retains_record_through_eviction() {
    let host = CpuHost::cooperative();
    let legacy = host.admit(&empty(), &request(), JobOptions::default()).unwrap();
    let mut input = request();
    let mut lights = Vec::with_capacity(1);
    lights.push(super::super::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 });
    let capacity = lights.capacity();
    input.raster_lights = Some(lights);
    let retained = (capacity * std::mem::size_of::<super::super::DirectionalLightOptions>()) as u64;
    let task_fee = (8 * std::mem::size_of::<super::super::DirectionalLightOptions>()) as u64;
    let admitted = host.admit(&empty(), &input, JobOptions::default()).unwrap();
    assert_eq!(admitted.estimated_total_bytes, legacy.estimated_total_bytes + task_fee);
    let job = host.submit(id(830, 1), 1, empty(), input).unwrap();
    assert_eq!(host.admitted_bytes(), admitted.estimated_total_bytes + retained);
    let status = job.status().unwrap();
    assert_eq!(status.request.raster_lights.as_ref().unwrap().len(), 1);
    assert!(status.request.raster_lights.as_ref().unwrap().capacity() <= 8);
    assert_eq!(job.0.request.raster_lights.as_ref().unwrap().capacity(), capacity);
    // Caller-owned status metadata is outside the host lease lifetime.
    drop(status);
    drain(&host);
    let held_frame = job.frame().unwrap().unwrap();
    job.0.data.lock().unwrap().frame = None;
    assert!(host.admitted_bytes() > retained);
    drop(held_frame);
    assert_eq!(host.admitted_bytes(), retained);
    drop(job);
    assert_eq!(host.admitted_bytes(), 0);
    // These counters test fees/lifetimes; declaration-order source review separately
    // verifies request payload destruction precedes release of request credits.
}

#[test]
fn raster_rig_cancel_retains_only_original_request_until_last_handle() {
    let host = CpuHost::cooperative();
    let mut input = request();
    input.raster_lights = Some(vec![super::super::DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 2.0 }]);
    let retained = input.raster_lights_retained_bytes().unwrap();
    let job = host.submit(id(831, 1), 1, empty(), input).unwrap();
    job.cancel().unwrap();
    drain(&host);
    assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
    assert_eq!(host.admitted_bytes(), retained);
    drop(job);
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn registered_and_interactive_routes_refuse_even_empty_raster_rigs_without_work() {
    let (registered, _) = probe_host(1024);
    let mut input = request();
    input.raster_lights = Some(Vec::new());
    assert_eq!(registered.admit(&empty(), &input, JobOptions::default()).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.submit(id(832, 1), 1, empty(), input.clone()).err().unwrap().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.admitted_bytes(), 0);
    assert!(!registered.pump(1).unwrap());
    assert!(super::super::interactive::cpu_request(&input).is_err());
    input.integrator = Integrator::Raytrace;
    assert!(super::super::interactive::cpu_request(&input).is_err());
}

mod interactive {
    use super::*;
    use crate::job::{RaytraceOptions, interactive::*};
    use std::time::Duration;

    fn cpu() -> ImageRequest {
        ImageRequest { integrator: Integrator::Raytrace, raytrace: Some(RaytraceOptions::default()), ..request() }
    }
    fn revision(visual_revision: u64) -> VisualRevision {
        VisualRevision { scene_generation: 19, visual_revision }
    }
    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn preview_fits_landscape_portrait_square_and_small_outputs_without_mutating_final_intent() {
        for (width, height, expected) in
            [(3840, 2160, (1024, 576)), (2160, 3840, (576, 1024)), (4096, 4096, (1024, 1024)), (81, 53, (81, 53)), (u32::MAX, 1, (1024, 1))]
        {
            let mut source = cpu();
            source.width = width;
            source.height = height;
            source.camera.roll = 0.37;
            source.background = [19, 31, 97];
            source.studio_weights = Some([0.1, 0.2, 0.9]);
            source.raytrace = Some(RaytraceOptions { primary_samples: 16, exposure_ev: -2.0, shadows: false, ..Default::default() });
            let before = serde_json::to_value(&source).unwrap();
            let preview = cpu_request(&source).unwrap();
            assert_eq!((preview.width, preview.height), expected);
            assert_eq!(preview.camera, source.camera);
            assert_eq!(preview.background, source.background);
            assert_eq!(preview.studio_weights, source.studio_weights);
            let options = preview.raytrace.unwrap();
            assert_eq!(options.progressive, Some(crate::job::CpuProgressiveOptions { samples_per_pixel: 16, seed: 0 }));
            assert_eq!(options.primary_samples, 1);
            assert_eq!(options.exposure_ev, -2.0);
            assert!(!options.shadows);
            assert_eq!(serde_json::to_value(source).unwrap(), before);
        }
    }

    #[test]
    fn preview_refuses_incompatible_transport_and_invalid_input_without_erasing_it() {
        let mut source = cpu();
        // A valid fixed thin sheet remains deliberately unsupported by progressive.
        // Area/LED now has its positive preservation case below.
        source.raytrace.as_mut().unwrap().dielectrics = vec![crate::job::DielectricOptions {
            body_index: 0,
            body_name: "Original sheet".into(),
            boundary: crate::job::DielectricBoundary::ThinSheet,
            ior: 1.5,
            absorption_per_mm: [0.0; 3],
            thickness_mm: 1.0,
        }];
        source.raytrace.as_ref().unwrap().validate().unwrap();
        let before = serde_json::to_value(&source).unwrap();
        assert!(cpu_request(&source).is_err());
        assert_eq!(serde_json::to_value(&source).unwrap(), before);
        for source in [
            ImageRequest { transparent_background: true, ..cpu() },
            ImageRequest { width: 0, ..cpu() },
            request(),
            ImageRequest { raytrace: Some(RaytraceOptions { primary_samples: 3, ..Default::default() }), ..cpu() },
        ] {
            assert!(cpu_request(&source).is_err());
        }
    }

    #[test]
    fn preview_preserves_rectangle_and_led_sources_and_all_nonsampling_options() {
        use crate::job::{
            CpuProgressiveOptions, DirectionalLightOptions, PunctualLightOptions, RaytraceEnvironmentOptions, RectangleLightOptions, ReflectionBody,
            ReflectionMode, ReflectionOptions, led_strip::LedStripOptions,
        };
        let rectangle = RectangleLightOptions {
            center_mm: [0.0, 0.0, 10.0],
            edge_u_mm: [2.0, 0.0, 0.0],
            edge_v_mm: [0.0, -2.0, 0.0],
            color: [1.0, 0.5, 0.25],
            intensity: 2.0,
        };
        let led = LedStripOptions {
            center_mm: [5.0, 0.0, 10.0],
            length_mm: 8.0,
            width_mm: 0.5,
            length_axis: [1.0, 0.0, 0.0],
            width_axis: [0.0, -1.0, 0.0],
            color: [0.25, 0.5, 1.0],
            intensity: 3.0,
        };
        for area_samples in [1, 4, 16, 64] {
            let mut source = cpu();
            source.width = 3840;
            source.height = 2160;
            source.camera.roll = 0.37;
            source.background = [19, 31, 97];
            let mut options = RaytraceOptions {
                primary_samples: 16,
                exposure_ev: 1.25,
                shadows: false,
                lights: Some(vec![DirectionalLightOptions { direction: [0.0, 0.0, 1.0], color: [1.0; 3], intensity: 1.0 }]),
                punctual_lights: Some(vec![PunctualLightOptions {
                    position_mm: [0.0, 0.0, 20.0],
                    color: [1.0; 3],
                    intensity: 2.0,
                    reference_distance_mm: 10.0,
                    minimum_distance_mm: 0.01,
                    cone: None,
                }]),
                area_lights: Some(vec![rectangle.clone()]),
                area_samples: Some(area_samples),
                seed: Some(73),
                environment: Some(RaytraceEnvironmentOptions { color: [0.2, 0.3, 0.4], intensity: 2.0 }),
                reflections: Some(ReflectionOptions {
                    bodies: vec![ReflectionBody { body: "Subject".into(), mode: ReflectionMode::RoughSpecular }],
                    samples: 16,
                    max_bounces: 3,
                }),
                ..Default::default()
            };
            led.append_to(&mut options, area_samples).unwrap();
            options.validate().unwrap();
            source.raytrace = Some(options);
            let before = serde_json::to_value(&source).unwrap();
            let mut expected = source.clone();
            expected.width = 1024;
            expected.height = 576;
            let expected_options = expected.raytrace.as_mut().unwrap();
            expected_options.primary_samples = 1;
            expected_options.progressive = Some(CpuProgressiveOptions { samples_per_pixel: 16, seed: 0 });
            expected_options.reflections.as_mut().unwrap().samples = 1;

            let preview = cpu_request(&source).unwrap();
            assert_eq!(
                serde_json::to_value(&preview).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "all request fields must survive except the explicitly chosen preview envelope"
            );
            assert_eq!(preview.raytrace, expected.raytrace);
            assert!(preview.environment.is_none() && preview.textures.is_none());
            let preview_options = preview.raytrace.as_ref().unwrap();
            assert_eq!(preview_options.area_lights.as_ref().unwrap().as_slice(), &[rectangle.clone(), led.rectangle().unwrap()]);
            assert_eq!(preview_options.area_samples, Some(area_samples));
            assert_eq!(preview_options.seed, Some(73), "camera seed zero must not replace the area-pattern seed");
            assert_eq!(serde_json::to_value(&source).unwrap(), before, "the final request remains immutable");
        }
    }

    #[test]
    fn explicit_enable_settles_edits_and_keeps_only_one_capture_even_without_cad_revision_changes() {
        let mut interactive = Interactive::new(9001);
        interactive.edit(revision(1), ms(0)).unwrap();
        assert_eq!(interactive.state().unwrap(), State::Off);
        assert!(interactive.capture(ms(1000)).unwrap().is_none());
        interactive.enable(revision(1), ms(1000)).unwrap();
        assert!(interactive.capture(ms(1199)).unwrap().is_none());
        interactive.edit(revision(2), ms(1199)).unwrap();
        assert!(interactive.capture(ms(1398)).unwrap().is_none());
        let b = interactive.capture(ms(1399)).unwrap().unwrap();
        assert_eq!(b.revision(), revision(2));
        assert!(interactive.accepts_capture(b));
        assert!(interactive.capture(ms(9999)).unwrap().is_none());
        interactive.edit(revision(3), ms(1400)).unwrap();
        assert!(!interactive.accepts_capture(b));
        // Delayed failure of B cannot poison C.
        interactive.capture_failed(b, JobError::new(ErrorCode::InvalidScene, "old capture"));
        assert_eq!(interactive.state().unwrap(), State::Updating);
        let c = interactive.capture(ms(1600)).unwrap().unwrap();
        interactive.capture_failed(c, JobError::new(ErrorCode::InvalidScene, "current capture"));
        assert!(!interactive.accepts_capture(c));
        assert_eq!(interactive.state().unwrap(), State::Error);
        assert_eq!(interactive.error().unwrap().message, "current capture");
        assert!(interactive.capture(ms(9999)).unwrap().is_none(), "no automatic failure retry loop");
        interactive.resume(ms(2000)).unwrap();
        assert!(interactive.capture(ms(2200)).unwrap().is_some());
    }

    #[test]
    fn pause_edits_final_priority_workspace_departure_and_clock_regression_do_not_start_jobs() {
        let mut interactive = Interactive::new(9002);
        interactive.enable(revision(1), ms(1000)).unwrap();
        assert!(interactive.capture(ms(999)).unwrap().is_none());
        interactive.pause().unwrap();
        interactive.edit(revision(2), ms(2000)).unwrap();
        assert_eq!(interactive.state().unwrap(), State::Paused);
        assert!(interactive.capture(ms(3000)).unwrap().is_none());
        interactive.set_final_active(true, ms(3000)).unwrap();
        interactive.resume(ms(3000)).unwrap();
        assert!(interactive.capture(ms(4000)).unwrap().is_none());
        interactive.set_final_active(false, ms(4000)).unwrap();
        assert!(interactive.capture(ms(4199)).unwrap().is_none());
        assert_eq!(interactive.capture(ms(4200)).unwrap().unwrap().revision(), revision(2));
        interactive.disable().unwrap();
        assert_eq!(interactive.state().unwrap(), State::Off);
        assert!(interactive.capture(ms(9999)).unwrap().is_none());
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    fn host() -> CpuHost {
        // Real native admission/worker steps, explicitly pumped like the existing
        // checkpoint tests. No test clock sleeps, mock renderer or OS race.
        ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }))
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn superseded_jobs_are_cancelled_and_foreign_jobs_remain_untouched() {
        let host = host();
        let mut interactive = Interactive::new(9003);
        interactive.enable(revision(1), ms(0)).unwrap();
        let a = interactive.capture(ms(200)).unwrap().unwrap();
        let first = host.submit(id(9003, 1), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(interactive.attach(a, first.clone()).unwrap());
        interactive.edit(revision(2), ms(201)).unwrap();
        assert_eq!(first.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(interactive.current().is_none());
        let b = interactive.capture(ms(401)).unwrap().unwrap();
        // A delayed A admission must carry A's identity, not the pending B's.
        let late = host.submit(a.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert_eq!(late.id(), a.job_id());
        assert_ne!(late.id(), b.job_id());
        assert!(!interactive.attach(a, late.clone()).unwrap());
        assert_eq!(late.status().unwrap().progress.phase, Phase::Cancelled);
        // Drain cancelled queue entries before admitting another real job.
        while let Some(work) = host.0.shared.queue.lock().unwrap().pop_front() {
            drop(work);
        }
        let foreign = host.submit(id(9999, 1), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(!interactive.attach(b, foreign.clone()).unwrap());
        assert_eq!(foreign.status().unwrap().progress.phase, Phase::Queued);
        assert_eq!(interactive.state().unwrap(), State::Error);
        interactive.disable().unwrap();
        drop(interactive);
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let mut running = Running::begin(work).unwrap();
        for _ in 0..100_000 {
            if !running.advance(16).unwrap() {
                break;
            }
        }
        assert_eq!(foreign.status().unwrap().progress.phase, Phase::Completed, "the foreign job can still finish after our controller is closed");
        assert!(foreign.frame().unwrap().is_some());
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn progressive_checkpoints_reach_target_and_final_suspension_releases_owned_work() {
        let host = host();
        let mut interactive = Interactive::new(9004);
        interactive.enable(revision(1), ms(0)).unwrap();
        let ticket = interactive.capture(ms(200)).unwrap().unwrap();
        let job = host.submit(id(9004, 1), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(interactive.attach(ticket, job.clone()).unwrap());
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let mut running = Running::begin(work).unwrap();
        let mut seen = Vec::new();
        for _ in 0..100_000 {
            let more = running.advance(16).unwrap();
            if let Some(preview) = interactive.current().unwrap().preview().unwrap() {
                let spp = preview.info.samples_per_pixel;
                if seen.last() != Some(&spp) {
                    seen.push(spp);
                }
            }
            if !more {
                break;
            }
        }
        assert_eq!(seen, [1, 2, 4, 8]);
        assert_eq!(interactive.state().unwrap(), State::TargetReached);
        assert!(job.frame().unwrap().is_some());
        assert!(!interactive.attach(ticket, job.clone()).unwrap(), "duplicate acknowledgement cannot restart or cancel the accepted job");
        interactive.set_final_active(true, ms(300)).unwrap();
        assert_eq!(interactive.state().unwrap(), State::Paused);
        assert!(interactive.current().is_none());
        assert!(interactive.capture(ms(9999)).unwrap().is_none());
        drop(running);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn same_scene_stale_serial_generation_and_delayed_success_after_failure_are_rejected() {
        for mismatch in 0..3 {
            let host = host();
            let mut interactive = Interactive::new(9005);
            interactive.enable(revision(1), ms(0)).unwrap();
            let ticket = interactive.capture(ms(200)).unwrap().unwrap();
            let mut job_id = ticket.job_id();
            let mut scene_generation = ticket.revision().scene_generation;
            match mismatch {
                0 => job_id.serial += 1,
                1 => job_id.generation += 1,
                _ => scene_generation += 1,
            }
            let job = host.submit(job_id, scene_generation, empty(), cpu_request(&cpu()).unwrap()).unwrap();
            assert!(!interactive.attach(ticket, job.clone()).unwrap());
            assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
            assert_eq!(interactive.state().unwrap(), State::Error);
        }
        let host = host();
        let mut interactive = Interactive::new(9006);
        interactive.enable(revision(1), ms(0)).unwrap();
        let ticket = interactive.capture(ms(200)).unwrap().unwrap();
        interactive.capture_failed(ticket, JobError::new(ErrorCode::InvalidScene, "capture failed"));
        let late = host.submit(ticket.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(!interactive.attach(ticket, late.clone()).unwrap());
        assert_eq!(late.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(interactive.current().is_none());
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn delayed_ticket_cannot_cancel_the_current_handle_but_still_cancels_its_obsolete_job() {
        let host = host();
        let mut interactive = Interactive::new(9007);
        interactive.enable(revision(1), ms(0)).unwrap();
        let old = interactive.capture(ms(200)).unwrap().unwrap();
        interactive.edit(revision(2), ms(201)).unwrap();
        let current = interactive.capture(ms(401)).unwrap().unwrap();
        let job = host.submit(current.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(interactive.attach(current, job.clone()).unwrap());
        assert!(!interactive.attach(old, job.clone()).unwrap());
        assert_eq!(job.status().unwrap().progress.phase, Phase::Queued);
        let obsolete = host.submit(old.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(!interactive.attach(old, obsolete.clone()).unwrap());
        assert_eq!(obsolete.status().unwrap().progress.phase, Phase::Cancelled);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Queued);
        assert_eq!(interactive.current().unwrap().id(), current.job_id());
        drop(interactive);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn stale_ticket_cannot_cancel_current_job_before_acknowledgement() {
        let host = host();
        let mut interactive = Interactive::new(9008);
        interactive.enable(revision(1), ms(0)).unwrap();
        let old = interactive.capture(ms(200)).unwrap().unwrap();
        interactive.edit(revision(2), ms(201)).unwrap();
        let current = interactive.capture(ms(401)).unwrap().unwrap();
        let job = host.submit(current.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        // B is admitted but not yet acknowledged. A delayed A callback has no
        // authority to cancel B, including before current() can return a handle.
        assert!(interactive.current().is_none());
        assert!(interactive.accepts_capture(current));
        assert!(!interactive.accepts_capture(old));
        assert!(!interactive.attach(old, job.clone()).unwrap());
        assert_eq!(job.status().unwrap().progress.phase, Phase::Queued);
        assert!(interactive.current().is_none());
        assert_eq!(interactive.state().unwrap(), State::Updating);
        assert!(interactive.attach(current, job.clone()).unwrap());
        assert!(!interactive.accepts_capture(current));
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let mut running = Running::begin(work).unwrap();
        for _ in 0..100_000 {
            if !running.advance(16).unwrap() {
                break;
            }
        }
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        assert!(job.frame().unwrap().is_some());
        assert_eq!(interactive.state().unwrap(), State::TargetReached);
        drop(running);
        drop(interactive);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn distinct_same_id_records_are_cancelled_but_current_handle_clones_finish() {
        let host = host();
        let mut interactive = Interactive::new(9009);
        interactive.enable(revision(1), ms(0)).unwrap();
        let old = interactive.capture(ms(200)).unwrap().unwrap();
        interactive.edit(revision(2), ms(201)).unwrap();
        let current = interactive.capture(ms(401)).unwrap().unwrap();
        let accepted = host.submit(current.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
        assert!(interactive.attach(current, accepted.clone()).unwrap());
        for callback in [old, current] {
            // The host accepts an explicit JobId; submitting twice creates two
            // records. Metadata equality is not actual admitted-work identity.
            let duplicate = host.submit(current.job_id(), 19, empty(), cpu_request(&cpu()).unwrap()).unwrap();
            assert_eq!(accepted.id(), duplicate.id());
            assert!(!Arc::ptr_eq(&accepted.0, &duplicate.0));
            assert!(!interactive.attach(callback, duplicate.clone()).unwrap());
            assert_eq!(duplicate.status().unwrap().progress.phase, Phase::Cancelled);
            assert_eq!(accepted.status().unwrap().progress.phase, Phase::Queued);
            assert!(Arc::ptr_eq(&interactive.current().unwrap().0, &accepted.0));
            // A second callback for the original Arc is harmless for both an
            // obsolete ticket and the already acknowledged current ticket.
            assert!(!interactive.attach(callback, accepted.clone()).unwrap());
            assert_eq!(accepted.status().unwrap().progress.phase, Phase::Queued);
        }
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let mut running = Running::begin(work).unwrap();
        for _ in 0..100_000 {
            if !running.advance(16).unwrap() {
                break;
            }
        }
        assert_eq!(accepted.status().unwrap().progress.phase, Phase::Completed);
        assert!(accepted.frame().unwrap().is_some());
        assert_eq!(interactive.state().unwrap(), State::TargetReached);
        while let Some(work) = host.0.shared.queue.lock().unwrap().pop_front() {
            assert_eq!(work.handle.status().unwrap().progress.phase, Phase::Cancelled);
            drop(work);
        }
        drop(running);
        drop(interactive);
        drop(accepted);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod progressive_checkpoints {
    use super::*;
    use crate::job::{CpuProgressiveOptions, RaytraceOptions};

    // The production native admission/Running path, manually advanced so tests
    // can stop exactly between hashing and the cancellation/publication lock.
    fn fixture(samples: u32) -> (CpuHost, JobHandle, Running) {
        let host = ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }));
        let request = ImageRequest {
            integrator: Integrator::Raytrace,
            raytrace: Some(RaytraceOptions {
                progressive: Some(CpuProgressiveOptions { samples_per_pixel: samples, seed: 123 }),
                lights: Some(Vec::new()),
                ..Default::default()
            }),
            width: 4,
            height: 4,
            ..request()
        };
        let expected = admit(&empty(), &request).unwrap().estimated_total_bytes;
        let job = host.submit(id(7300, 1), 19, empty(), request).unwrap();
        assert_eq!(host.admitted_bytes(), expected);
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let running = Running::begin(work).unwrap();
        (host, job, running)
    }

    fn before_preview(running: &mut Running) {
        for _ in 0..100_000 {
            if running.pending_png.as_ref().is_some_and(|p| p.preview_samples.is_some()) {
                return;
            }
            assert!(running.advance(1).unwrap());
        }
        panic!("small CPU job did not reach its checkpoint");
    }

    #[test]
    fn checkpoints_transfer_png_allocations_and_keep_every_external_lease() {
        let (host, job, mut running) = fixture(3);
        let initial = host.admitted_bytes();
        let mut held = Vec::new();
        for (sequence, samples) in [(1, 1), (2, 2)] {
            before_preview(&mut running);
            let pointer = running.pending_png.as_ref().unwrap().png.as_ptr();
            assert!(running.task.is_some(), "a preview must retain film, BVH and task state");
            assert!(running.advance(1).unwrap());
            let preview = job.preview().unwrap().unwrap();
            assert_eq!(preview.png().as_ptr(), pointer, "PNG ownership moves without cloning its allocation");
            assert_eq!(preview.info.frame_sequence, sequence);
            assert_eq!(preview.info.samples_per_pixel, samples);
            assert_eq!(preview.info.frame.job, job.id());
            assert_eq!(preview.info.frame.scene_generation, 19);
            assert_eq!(preview.info.frame.png_sha256, format!("{:x}", Sha256::digest(preview.png())));
            assert!(job.frame().unwrap().is_none());
            assert!(job.status().unwrap().frame.is_none());
            assert_eq!(host.admitted_bytes(), initial, "splitting a prepaid lease does not change pool usage");
            held.push(preview);
        }
        let mut complete = false;
        for _ in 0..100_000 {
            if !running.advance(1).unwrap() {
                complete = true;
                break;
            }
        }
        assert!(complete);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        assert!(job.preview().unwrap().is_none());
        let frame = job.frame().unwrap().unwrap();
        assert_eq!(frame.info.job, job.id());
        let retained: u64 = held.iter().map(|p| p._reservation.bytes).sum();
        assert_eq!(host.admitted_bytes(), retained + frame._reservation.bytes);
        drop(running);
        drop(job);
        drop(frame);
        assert_eq!(host.admitted_bytes(), retained);
        while let Some(preview) = held.pop() {
            let before = host.admitted_bytes();
            let bytes = preview._reservation.bytes;
            drop(preview);
            assert_eq!(host.admitted_bytes(), before - bytes);
        }
        assert_eq!(host.admitted_bytes(), 0);
    }

    #[test]
    fn cancellation_before_or_after_digest_cannot_publish_a_late_preview() {
        for finish_digest in [false, true] {
            let (host, job, mut running) = fixture(3);
            before_preview(&mut running);
            assert!(running.advance(1).unwrap());
            let held = job.preview().unwrap().unwrap();
            before_preview(&mut running);
            if finish_digest {
                let pending = running.pending_png.as_mut().unwrap();
                while !pending.step(1, &job.0.cancel).unwrap() {}
                assert_eq!(pending.hashed, pending.png.len());
            }
            job.cancel().unwrap();
            assert!(job.preview().unwrap().is_none(), "cancel removes the provisional slot immediately");
            assert!(!running.guarded_step(1));
            assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
            assert!(job.frame().unwrap().is_none());
            drop(running);
            drop(job);
            assert_eq!(host.admitted_bytes(), held._reservation.bytes);
            assert_eq!(held.info.samples_per_pixel, 1);
            drop(held);
            assert_eq!(host.admitted_bytes(), 0);
        }
    }

    #[test]
    fn an_invalid_checkpoint_sequence_fails_without_replacing_the_old_snapshot() {
        let (host, job, mut running) = fixture(4);
        before_preview(&mut running);
        assert!(running.advance(1).unwrap());
        let held = job.preview().unwrap().unwrap();
        before_preview(&mut running);
        running.pending_png.as_mut().unwrap().preview_samples = Some(3);
        assert!(!running.guarded_step(1));
        assert_eq!(job.status().unwrap().progress.phase, Phase::Failed);
        assert!(job.preview().unwrap().is_none());
        assert!(job.frame().unwrap().is_none());
        assert_eq!(held.info.samples_per_pixel, 1);
        drop(running);
        drop(job);
        assert_eq!(host.admitted_bytes(), held._reservation.bytes);
        drop(held);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

fn request() -> ImageRequest {
    ImageRequest {
        raster_exposure_ev: 0.0,
        raster_lights: None,
        studio_weights: None,
        integrator: Default::default(),
        raytrace: None,
        textures: None,
        environment: None,
        camera: Default::default(),
        width: 16,
        height: 16,
        background: [40, 80, 120],
        transparent_background: false,
    }
}

#[test]
fn legacy_image_requests_default_to_raster_and_reject_unknown_integrators() {
    let mut value = serde_json::to_value(request()).unwrap();
    value.as_object_mut().unwrap().remove("integrator");
    let decoded: ImageRequest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.integrator, Integrator::Raster);
    value["integrator"] = serde_json::json!("gpu_pathtrace");
    assert!(serde_json::from_value::<ImageRequest>(value).is_err());
}

#[cfg(feature = "raytrace-cpu")]
#[test]
fn cooperative_raytrace_descriptor_matches_native_only_validation() {
    let host = CpuHost::cooperative();
    assert!(host.validate_integrator(Integrator::Raytrace).is_err());
    assert!(matches!(host.descriptor_for(Integrator::Raytrace).unwrap().availability, Availability::Unavailable { .. }));
    assert!(host.validate_integrator(Integrator::Raster).is_ok());
    assert!(!host.descriptor_for(Integrator::Raytrace).unwrap().profiles.iter().any(|p| p.features.contains(&crate::RenderFeature::Refraction)));
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
#[test]
fn native_raytrace_refraction_capability_resolves_only_preview_images() {
    let host = CpuHost::for_platform().unwrap();
    let descriptor = host.descriptor_for(Integrator::Raytrace).unwrap();
    assert_eq!(descriptor.availability, Availability::Available);
    assert!(descriptor.profiles.iter().any(|profile| profile.features.contains(&crate::RenderFeature::Refraction)));
    let mut backend = crate::builtin_backend();
    backend.executors = vec![descriptor];
    let mut request = crate::RenderRequest {
        backend: None,
        mode: RenderMode::Preview,
        operation: RenderOperation::Image,
        width: 1,
        height: 1,
        format: Some(RenderFormat::Png),
        features: vec![crate::RenderFeature::Refraction],
        policy: Default::default(),
    };
    let resolved = crate::resolve_request(std::slice::from_ref(&backend), &request).unwrap();
    assert_eq!(resolved.executor.as_str(), "cpu-raytrace");
    request.mode = RenderMode::Final;
    assert!(crate::resolve_request(std::slice::from_ref(&backend), &request).is_err());
    request.mode = RenderMode::Preview;
    backend.executors = vec![host.descriptor_for(Integrator::Raster).unwrap()];
    assert!(crate::resolve_request(&[backend], &request).is_err(), "CPU raster must not advertise physical refraction");
    assert_eq!(host.admitted_bytes(), 0, "capability discovery never starts a render");
}

#[cfg(not(feature = "raytrace-cpu"))]
#[test]
fn compiled_out_raytracing_never_enters_the_host_queue() {
    let host = CpuHost::cooperative();
    let mut request = request();
    request.integrator = Integrator::Raytrace;
    assert!(host.validate_integrator(Integrator::Raytrace).unwrap_err().message.contains("not compiled in"));
    assert!(host.descriptor_for(Integrator::Raytrace).is_err());
    let error = host.submit(JobId { session: 1, serial: 1, generation: 1 }, 0, empty(), request).err().unwrap();
    assert_eq!(error.code, ErrorCode::HostUnavailable);
    assert_eq!(host.admitted_bytes(), 0);
    assert!(!host.pump(1).unwrap());
}
fn empty() -> ProductScene {
    ProductScene { meshes: Vec::new(), bounds: Aabb3::EMPTY }
}
fn id(owner: u64, serial: u64) -> JobId {
    JobId { session: owner, serial, generation: serial }
}
fn drain(host: &CpuHost) {
    for _ in 0..10_000 {
        if !host.pump(4096).unwrap() {
            return;
        }
    }
    panic!("small cooperative jobs did not drain");
}

#[test]
fn queue_is_bounded_and_cancelled_entries_release_their_reservations() {
    let host = CpuHost::cooperative();
    let a = host.submit(id(10, 1), 100, empty(), request()).unwrap();
    let b = host.submit(id(20, 1), 200, empty(), request()).unwrap();
    let before = host.admitted_bytes();
    assert_eq!(host.submit(id(30, 1), 300, empty(), request()).err().unwrap().code, ErrorCode::QueueFull);
    assert_eq!(host.admitted_bytes(), before);
    assert_eq!(a.cancel().unwrap().progress.phase, Phase::Cancelled);
    let c = host.submit(id(30, 1), 300, empty(), request()).unwrap();
    drain(&host);
    assert_eq!(a.status().unwrap().progress.phase, Phase::Cancelled);
    for (handle, owner, generation) in [(&b, 20, 200), (&c, 30, 300)] {
        let frame = handle.frame().unwrap().unwrap();
        assert_eq!(frame.info.job, id(owner, 1));
        assert_eq!(frame.info.scene_generation, generation);
        assert_eq!(frame.info.executor.as_str(), "cpu-product");
        assert_eq!(image::load_from_memory(frame.png()).unwrap().width(), 16);
        assert_eq!(frame.info.png_bytes, frame.png().len() as u64);
        assert_eq!(frame.info.png_sha256, format!("{:x}", Sha256::digest(frame.png())));
        assert_eq!(handle.status().unwrap().frame.unwrap().png_sha256, frame.info.png_sha256);
    }
    assert!(host.admitted_bytes() > 0 && host.admitted_bytes() < before);
    let frame = b.frame().unwrap().unwrap();
    drop(b);
    drop(c);
    assert!(host.admitted_bytes() > 0, "an externally retained frame retains its lease");
    drop(frame);
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn cancellation_while_encoding_cannot_publish_a_late_frame() {
    let host = CpuHost::cooperative();
    let job = host.submit(id(1, 1), 8, empty(), request()).unwrap();
    for _ in 0..10_000 {
        host.pump(64).unwrap();
        let status = job.status().unwrap();
        if status.progress.phase == Phase::Encoding && status.progress.done > 0 {
            break;
        }
    }
    assert_eq!(job.status().unwrap().progress.phase, Phase::Encoding);
    assert_eq!(job.cancel().unwrap().progress.phase, Phase::Cancelling);
    drain(&host);
    assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
    assert!(job.frame().unwrap().is_none());
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn malformed_preparation_fails_the_job_without_stopping_the_host() {
    let host = CpuHost::cooperative();
    let scene = ProductScene {
        bounds: Aabb3::EMPTY,
        meshes: vec![crate::ProductMesh {
            name: "Broken".into(),
            mesh: Arc::new(Mesh { triangles: vec![[99, 99, 99]], ..Default::default() }),
            color: [0; 3],
            opacity: 1.0,
            surface: None,
            transmission: None,
            faces: Vec::new(),
        }],
    };
    let broken = host.submit(id(1, 1), 1, scene, request()).unwrap();
    drain(&host);
    let status = broken.status().unwrap();
    assert_eq!(status.progress.phase, Phase::Failed);
    assert_eq!(status.error.unwrap().code, ErrorCode::InvalidScene);
    assert!(status.frame.is_none(), "a failed preparation path has no cached PNG identity");
    assert!(broken.frame().unwrap().is_none());
    assert!(host.is_available());
    let healthy = host.submit(id(1, 2), 1, empty(), request()).unwrap();
    drain(&host);
    assert_eq!(healthy.status().unwrap().progress.phase, Phase::Completed);
}

#[test]
fn closing_the_host_cancels_active_and_queued_work() {
    let host = CpuHost::cooperative();
    let budget = host.0.shared.budget.clone();
    let active = host.submit(id(1, 1), 1, empty(), request()).unwrap();
    let queued = host.submit(id(2, 1), 1, empty(), request()).unwrap();
    host.pump(1).unwrap();
    drop(host);
    assert_eq!(active.status().unwrap().progress.phase, Phase::Cancelled);
    assert_eq!(queued.status().unwrap().progress.phase, Phase::Cancelled);
    assert_eq!(budget.used.load(Ordering::Acquire), 0);
}

#[test]
fn a_foreign_frame_is_rejected_even_if_a_broken_host_puts_it_in_the_result_slot() {
    let host = CpuHost::cooperative();
    let job = host.submit(id(1, 1), 1, empty(), request()).unwrap();
    drain(&host);
    {
        let mut data = job.0.data.lock().unwrap();
        let frame = Arc::get_mut(data.frame.as_mut().unwrap()).unwrap();
        frame.info.job.session = 2;
    }
    assert_eq!(job.frame().err().unwrap().code, ErrorCode::WrongOwner);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_job_finishes_without_any_poll_or_ui_pump() {
    let host = CpuHost::for_platform().unwrap();
    assert_eq!(host.kind(), HostKind::NativeWorker);
    let job = host.submit(id(1, 1), 12, empty(), request()).unwrap();
    // A generous deadlock watchdog, not a performance assertion.
    let status = job.wait_timeout(std::time::Duration::from_secs(60)).unwrap();
    assert_eq!(status.progress.phase, Phase::Completed);
    assert_eq!(job.frame().unwrap().unwrap().info.scene_generation, 12);
    let frame = job.frame().unwrap().unwrap();
    assert_eq!(frame.info.png_bytes, frame.png().len() as u64);
    assert_eq!(frame.info.png_sha256, format!("{:x}", Sha256::digest(frame.png())));
    assert_eq!(status.frame.unwrap().png_sha256, frame.info.png_sha256);
}

/// Isolate the existing integrator-to-host completion boundary with a real PNG
/// from the production encoder. This does not pretend to render the noisy pixels.
fn pending_digest_fixture() -> (CpuHost, JobHandle, Running, usize) {
    let host = CpuHost::cooperative();
    let mut request = request();
    request.width = 256;
    request.height = 128;
    let mut rgba = vec![255; request.width as usize * request.height as usize * 4];
    let mut seed = 0x6a09_e667u32;
    for pixel in rgba.as_chunks_mut::<4>().0 {
        for channel in pixel.iter_mut().take(3) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *channel = seed as u8;
        }
    }
    let mut encoder = super::super::encoding::PngEncoding::new(
        request.width,
        request.height,
        u64::from(request.width) * u64::from(request.height) * 8 + 65_536,
        CancelToken::default(),
    )
    .unwrap();
    let png = loop {
        if let Some(png) = encoder.step(&rgba).unwrap() {
            break png;
        }
    };
    assert!(png.len() > PNG_HASH_CHUNK_BYTES * PNG_HASH_CHUNKS_PER_ADVANCE);
    let length = png.len();
    let job = host.submit(id(61, 1), 99, empty(), request).unwrap();
    let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
    let mut running = Running::begin(work).unwrap();
    // Exactly the same state transition used when WorkTask returns Complete.
    running.task = None;
    running.pending_png = Some(PendingPng::new(png));
    (host, job, running, length)
}

#[test]
fn png_digest_is_incremental_and_keeps_the_original_allocation_and_lease_until_completion() {
    let (host, job, mut running, length) = pending_digest_fixture();
    let before = host.admitted_bytes();
    let pointer = running.pending_png.as_ref().unwrap().png.as_ptr();
    assert!(running.advance(0).unwrap());
    assert_eq!(running.pending_png.as_ref().unwrap().hashed, 0, "zero units cannot hash a chunk");
    assert_eq!(host.admitted_bytes(), before);
    assert!(job.frame().unwrap().is_none());
    assert!(running.advance(usize::MAX).unwrap());
    assert_eq!(running.pending_png.as_ref().unwrap().hashed, PNG_HASH_CHUNK_BYTES * PNG_HASH_CHUNKS_PER_ADVANCE);
    assert_eq!(host.admitted_bytes(), before, "hashing must retain the full original work lease");
    assert!(job.frame().unwrap().is_none());
    let mut previous = job.status().unwrap().progress.done;
    while running.advance(1).unwrap() {
        let progress = job.status().unwrap().progress;
        assert_eq!(progress.phase, Phase::Encoding);
        assert_eq!(progress.total, length as u64);
        assert_eq!(progress.done - previous, PNG_HASH_CHUNK_BYTES as u64);
        assert_eq!(host.admitted_bytes(), before);
        assert!(job.frame().unwrap().is_none());
        previous = progress.done;
    }
    assert!(running.task.is_none() && running.pending_png.is_none() && running.reservation.is_none());
    let status = job.status().unwrap();
    assert_eq!(status.progress.phase, Phase::Completed);
    let frame = job.frame().unwrap().unwrap();
    assert_eq!(frame.png().as_ptr(), pointer, "the original Vec moves into the retained frame without cloning");
    assert_eq!(frame.info.png_bytes, length as u64);
    assert_eq!(frame.info.png_sha256.len(), 64);
    assert!(frame.info.png_sha256.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
    assert_eq!(frame.info.png_sha256, format!("{:x}", Sha256::digest(frame.png())));
    assert_eq!(status.frame.unwrap().png_sha256, frame.info.png_sha256);
    let decoded = image::load_from_memory(frame.png()).unwrap().to_rgba8();
    assert_eq!(decoded.dimensions(), (256, 128));
    assert!(decoded.pixels().all(|p| p.0[3] == 255));
    assert!(decoded.pixels().any(|p| p.0[..3] != decoded.get_pixel(0, 0).0[..3]));
    let actual_metadata =
        std::mem::size_of::<Running>() + std::mem::size_of::<RenderFrame>() + frame.info.png_sha256.capacity() + 2 * crate::MAX_RENDER_ID_BYTES + 32;
    assert!(actual_metadata <= FRAME_METADATA_BYTES as usize, "digest Vec/offset/state and actual hex allocation must be admitted");
    assert_eq!(host.admitted_bytes(), frame.png.capacity() as u64 + FRAME_METADATA_BYTES);
    for _ in 0..3 {
        let current = job.status().unwrap();
        assert_eq!(current.frame.unwrap().png_sha256, frame.info.png_sha256);
        assert_eq!(frame.png().as_ptr(), pointer);
    }
    drop(running);
    drop(job);
    assert!(host.admitted_bytes() > 0);
    drop(frame);
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn cancellation_during_png_hashing_and_host_disposal_cannot_publish_a_frame() {
    for close_host in [false, true] {
        let (host, job, running, _) = pending_digest_fixture();
        let budget = host.0.shared.budget.clone();
        *host.0.running.lock().unwrap() = Some(running);
        assert!(host.pump(1).unwrap());
        assert!(host.0.running.lock().unwrap().as_ref().unwrap().pending_png.as_ref().unwrap().hashed > 0);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Encoding);
        assert!(job.frame().unwrap().is_none());
        if close_host {
            drop(host);
        } else {
            assert_eq!(job.cancel().unwrap().progress.phase, Phase::Cancelling);
            assert!(!host.pump(1).unwrap());
            drop(host);
        }
        assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(job.frame().unwrap().is_none());
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
}

#[test]
fn a_cancel_after_the_last_digest_chunk_still_wins_the_publication_lock() {
    let (host, job, mut running, _) = pending_digest_fixture();
    let pending = running.pending_png.as_mut().unwrap();
    while !pending.step(1, &job.0.cancel).unwrap() {}
    assert_eq!(pending.hashed, pending.png.len());
    job.cancel().unwrap();
    assert!(!running.guarded_step(1));
    assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
    assert!(job.frame().unwrap().is_none());
    drop(running);
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn png_digest_metadata_fits_the_existing_allowance_without_changing_admission() {
    // Fixed digest/offset state and the completed frame (including its bounded
    // hex string and backend/executor IDs) fit the existing 1024-byte allowance.
    assert!(std::mem::size_of::<PendingPng>() <= 512);
    assert!(
        std::mem::size_of::<Running>() + std::mem::size_of::<RenderFrame>() + 64 + 2 * crate::MAX_RENDER_ID_BYTES + 32
            <= FRAME_METADATA_BYTES as usize
    );
    let host = CpuHost::cooperative();
    let expected = admit(&empty(), &request()).unwrap().estimated_total_bytes;
    let job = host.submit(id(70, 1), 1, empty(), request()).unwrap();
    assert_eq!(host.admitted_bytes(), expected);
    assert!(job.status().unwrap().frame.is_none());
    job.cancel().unwrap();
    drain(&host);
    assert_eq!(host.admitted_bytes(), 0);
}

/// An executor whose task reports, as it is dropped, how much of the pool is admitted, and
/// completes after `steps` steps (or never, cancelled on its last one with `cancel_last`).
struct DropProbe {
    admitted_at_drop: Arc<AtomicU64>,
    budget: Arc<std::sync::OnceLock<Arc<Budget>>>,
    estimated: u64,
}
struct ProbeTask {
    admitted_at_drop: Arc<AtomicU64>,
    budget: Arc<std::sync::OnceLock<Arc<Budget>>>,
    steps: u32,
}
impl Drop for ProbeTask {
    fn drop(&mut self) {
        let used = self.budget.get().map_or(u64::MAX, |b| b.used.load(Ordering::Acquire));
        self.admitted_at_drop.store(used, Ordering::Release);
    }
}
impl ImageWork for ProbeTask {
    fn step(&mut self, _units: usize) -> Result<TaskPoll> {
        self.steps += 1;
        if self.steps < 3 {
            return Ok(TaskPoll::Pending(Progress { phase: Phase::Running, done: u64::from(self.steps), total: 3 }));
        }
        Ok(TaskPoll::Complete(vec![0u8; 100]))
    }
}
impl ImageExecutor for DropProbe {
    fn descriptor(&self) -> Result<ExecutorDescriptor> {
        CpuExecutor.descriptor()
    }
    fn admit(&self, _: &ProductScene, _: &ImageRequest, _: JobOptions) -> Result<Admission> {
        Ok(Admission {
            triangles: 0,
            face_appearances: 0,
            geometry_bytes: 0,
            pixel_scratch_bytes: self.estimated,
            preparation_bytes: 0,
            estimated_total_bytes: self.estimated,
            encoded_limit: 4096,
        })
    }
    fn begin(&self, _: ProductScene, _: ImageRequest, _: JobOptions, _: CancelToken) -> Result<Box<dyn ImageWork>> {
        Ok(Box::new(ProbeTask { admitted_at_drop: self.admitted_at_drop.clone(), budget: self.budget.clone(), steps: 0 }))
    }
}

fn probe_host(estimated: u64) -> (CpuHost, Arc<AtomicU64>) {
    let admitted_at_drop = Arc::new(AtomicU64::new(0));
    let budget = Arc::new(std::sync::OnceLock::new());
    let host = CpuHost::cooperative_with(Arc::new(DropProbe { admitted_at_drop: admitted_at_drop.clone(), budget: budget.clone(), estimated }));
    let _ = budget.set(host.0.shared.budget.clone());
    (host, admitted_at_drop)
}

#[test]
fn a_completed_task_is_dropped_while_its_full_reservation_still_pays_for_it() {
    let estimated = 1 << 20;
    let (host, admitted_at_drop) = probe_host(estimated);
    let job = host.submit(id(1, 1), 1, empty(), request()).unwrap();
    drain(&host);
    assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
    // While the task (and its scratch) was being dropped, its whole charge was still held.
    assert_eq!(admitted_at_drop.load(Ordering::Acquire), estimated);
    // Afterwards only the PNG's lease remains, and the frame keeps it.
    let frame = job.frame().unwrap().unwrap();
    assert!(host.admitted_bytes() > 0 && host.admitted_bytes() < estimated, "{}", host.admitted_bytes());
    drop(job);
    drop(frame);
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn a_cancel_on_the_final_step_publishes_no_frame_and_drops_the_task() {
    let estimated = 1 << 20;
    let (host, admitted_at_drop) = probe_host(estimated);
    let job = host.submit(id(1, 1), 1, empty(), request()).unwrap();
    // Two pending steps, then cancel before the step that would complete.
    host.pump(1).unwrap();
    host.pump(1).unwrap();
    job.cancel().unwrap();
    drain(&host);
    assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
    assert!(job.frame().unwrap().is_none());
    assert_eq!(admitted_at_drop.load(Ordering::Acquire), estimated, "the task went before its charge");
    assert_eq!(host.admitted_bytes(), 0);
}

#[test]
fn cpu_and_registered_hosts_keep_separate_job_limits() {
    let cpu = CpuHost::cooperative();
    let (registered, _) = probe_host(400 * 1024 * 1024);
    assert_eq!(cpu.job_limit_bytes(), 384 * 1024 * 1024);
    assert_eq!(registered.job_limit_bytes(), 512 * 1024 * 1024);
    assert_eq!(cpu.pool_limit_bytes(), registered.pool_limit_bytes());
    assert_eq!(registered.pool_limit_bytes(), 512 * 1024 * 1024);
    assert_eq!(registered.admit(&empty(), &request(), JobOptions::default()).unwrap().estimated_total_bytes, 400 * 1024 * 1024);
    assert_eq!(admit_request_sum(400 * 1024 * 1024, 0).unwrap_err().code, ErrorCode::LimitExceeded);
    let (over_budget, _) = probe_host(512 * 1024 * 1024 + 1);
    assert_eq!(over_budget.admit(&empty(), &request(), JobOptions::default()).unwrap_err().code, ErrorCode::LimitExceeded);
    assert_eq!(registered.admitted_bytes(), 0);
}

#[test]
fn registered_hosts_refuse_cpu_integrators_and_options_before_reservation() {
    let (registered, _) = probe_host(1024 * 1024);
    let mut raytrace = request();
    raytrace.integrator = Integrator::Raytrace;
    assert_eq!(registered.validate_integrator(Integrator::Raytrace).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.descriptor_for(Integrator::Raytrace).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.admit(&empty(), &raytrace, JobOptions::default()).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.submit(id(250, 1), 1, empty(), raytrace).err().unwrap().code, ErrorCode::InvalidRequest);
    let mut options = request();
    options.raytrace = Some(super::super::RaytraceOptions::default());
    assert_eq!(registered.admit(&empty(), &options, JobOptions::default()).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(registered.admitted_bytes(), 0);
    assert!(!registered.pump(1).unwrap());
}

#[test]
fn environment_payload_is_owned_by_admitted_work_not_retained_status() {
    struct ResourceExecutor;
    impl ImageExecutor for ResourceExecutor {
        fn descriptor(&self) -> Result<ExecutorDescriptor> {
            let mut descriptor = CpuExecutor.descriptor()?;
            descriptor.profiles[0].features.push(crate::RenderFeature::Hdri);
            Ok(descriptor)
        }
        fn admit(&self, scene: &ProductScene, request: &ImageRequest, _: JobOptions) -> Result<Admission> {
            let mut admission = admit(scene, &ImageRequest { environment: None, textures: None, ..request.clone() })?;
            admission.estimated_total_bytes += request.environment.as_ref().map_or(0, |source| source.admitted_bytes());
            Ok(admission)
        }
        fn begin(&self, scene: ProductScene, mut request: ImageRequest, _: JobOptions, cancel: CancelToken) -> Result<Box<dyn ImageWork>> {
            request.environment = None;
            Ok(Box::new(ImageTask::new(scene, request, cancel)?))
        }
    }
    for cancelled in [false, true] {
        let host = ImageHost(Arc::new(Owner {
            executor: Some(Arc::new(ResourceExecutor)),
            shared: ImageHost::shared(MEMORY_LIMIT),
            kind: HostKind::Cooperative,
            running: Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            _worker: None,
        }));
        let source = Arc::new(crate::lighting::environment::EnvironmentSource::sky(Default::default(), 0.0, 1.0).unwrap());
        let weak = Arc::downgrade(&source);
        let mut input = request();
        input.environment = Some(source);
        let job = host.submit(id(1, 1), 1, empty(), input).unwrap();
        let status = job.status().unwrap();
        assert!(status.request.environment.is_none());
        assert!(matches!(status.environment.as_ref(), Some(JobEnvironmentInfo::Gpu(info)) if info.kind == "original_gradient_sky"));
        assert!(weak.upgrade().is_some(), "queuedwork owns source");
        if cancelled {
            job.cancel().unwrap();
        }
        drain(&host);
        assert!(weak.upgrade().is_none(), "completed/cancelled work releases captured source despite retained status/job");
        assert_eq!(job.status().unwrap().progress.phase, if cancelled { Phase::Cancelled } else { Phase::Completed });
    }
}

#[test]
fn texture_payload_is_owned_by_admitted_work_not_retained_status() {
    struct ResourceExecutor;
    impl ImageExecutor for ResourceExecutor {
        fn descriptor(&self) -> Result<ExecutorDescriptor> {
            let mut descriptor = CpuExecutor.descriptor()?;
            descriptor.profiles[0].features.push(crate::RenderFeature::Hdri);
            Ok(descriptor)
        }
        fn admit(&self, scene: &ProductScene, request: &ImageRequest, _: JobOptions) -> Result<Admission> {
            let mut admission = admit(scene, &ImageRequest { environment: None, textures: None, ..request.clone() })?;
            admission.estimated_total_bytes += request.textures.as_ref().map_or(0, |source| source.working_bytes());
            Ok(admission)
        }
        fn begin(&self, scene: ProductScene, mut request: ImageRequest, _: JobOptions, cancel: CancelToken) -> Result<Box<dyn ImageWork>> {
            request.textures = None;
            Ok(Box::new(ImageTask::new(scene, request, cancel)?))
        }
    }
    for cancelled in [false, true] {
        let host = ImageHost(Arc::new(Owner {
            executor: Some(Arc::new(ResourceExecutor)),
            shared: ImageHost::shared(MEMORY_LIMIT),
            kind: HostKind::Cooperative,
            running: Mutex::new(None),
            #[cfg(not(target_arch = "wasm32"))]
            _worker: None,
        }));
        use crate::job::textures::{PlanarFrame, RuntimeTextures, TextureBinding, TextureMap};
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes).write_image(&[128, 128, 255, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
        let source = Arc::new(
            RuntimeTextures::new(vec![TextureBinding {
                body_index: 0,
                body_name: "unused test resource".into(),
                frame: PlanarFrame { origin: [0.; 3], u: [1., 0., 0.], v: [0., 1., 0.] },
                normal_strength: 1.,
                base_color: Some(TextureMap::new(bytes, crate::texture_input::MapRole::BaseColor).unwrap()),
                normal: None,
                roughness: None,
                metallic: None,
                base_color_mode: Default::default(),
                flip_v: false,
            }])
            .unwrap(),
        );
        let weak = Arc::downgrade(&source);
        let mut input = request();
        input.textures = Some(source);
        let job = host.submit(id(1, 1), 1, empty(), input).unwrap();
        let status = job.status().unwrap();
        assert!(status.request.textures.is_none());
        assert_eq!(status.textures.as_ref().unwrap().bodies, 1);
        assert!(weak.upgrade().is_some(), "queuedwork owns source");
        if cancelled {
            job.cancel().unwrap();
        }
        drain(&host);
        assert!(weak.upgrade().is_none(), "completed/cancelled work releases captured source despite retained status/job");
        assert_eq!(job.status().unwrap().progress.phase, if cancelled { Phase::Cancelled } else { Phase::Completed });
    }
}

#[test]
fn cpu_film_snapshots_refuse_registered_executors_without_acquiring_a_lease() {
    let (host, _) = probe_host(1 << 20);
    let failure = host.reserve_film_snapshot(1024).err().unwrap();
    assert_eq!(failure.code, ErrorCode::InvalidRequest);
    assert!(failure.message.contains("builtin CPU host"));
    assert_eq!(host.admitted_bytes(), 0);
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32"), target_pointer_width = "64"))]
#[test]
fn native_cpu_envelopes_do_not_expand_registered_or_cooperative_pools() {
    use super::super::{CpuBudget, NATIVE_CPU_POOL_BYTES, RaytraceOptions};
    let cpu = CpuHost::for_platform().unwrap();
    let cooperative = CpuHost::cooperative();
    let (registered, _) = probe_host(1024);
    assert_eq!(cpu.pool_limit_bytes(), NATIVE_CPU_POOL_BYTES);
    assert_eq!(cooperative.pool_limit_bytes(), 512 * 1024 * 1024);
    assert_eq!(registered.pool_limit_bytes(), 512 * 1024 * 1024);
    assert!(cpu.check_request_sum(400 * 1024 * 1024, 0, CpuBudget::Compact).is_err());
    assert!(cpu.check_request_sum(400 * 1024 * 1024, 0, CpuBudget::Studio).is_ok());
    assert!(cpu.check_request_sum(u64::MAX, 1, CpuBudget::Workstation).is_err());
    let mut r = request();
    r.integrator = Integrator::Raytrace;
    r.width = 3840;
    r.height = 2160;
    r.raytrace = Some(RaytraceOptions { budget: CpuBudget::Studio, ..Default::default() });
    assert_eq!(cpu.descriptor_for_request(&r).unwrap().limits, CpuBudget::Studio.image_limits(false));
    r.raytrace.as_mut().unwrap().budget = CpuBudget::Compact;
    assert!(cpu.descriptor_for_request(&r).is_err());
    assert_eq!(cpu.admitted_bytes(), 0);
}

#[cfg(all(not(target_arch = "wasm32"), target_pointer_width = "64"))]
#[test]
fn registered_large_pool_is_explicit_finite_and_accounted() {
    struct Envelope(u64);
    impl ImageExecutor for Envelope {
        fn memory_limit_bytes(&self) -> u64 {
            self.0
        }
        fn descriptor(&self) -> Result<ExecutorDescriptor> {
            CpuExecutor.descriptor()
        }
        fn admit(&self, _: &ProductScene, _: &ImageRequest, _: JobOptions) -> Result<Admission> {
            Ok(Admission {
                triangles: 0,
                face_appearances: 0,
                geometry_bytes: 0,
                pixel_scratch_bytes: 0,
                preparation_bytes: 0,
                estimated_total_bytes: 600 * 1024 * 1024,
                encoded_limit: 4096,
            })
        }
        fn begin(&self, _: ProductScene, _: ImageRequest, _: JobOptions, _: CancelToken) -> Result<Box<dyn ImageWork>> {
            Err(JobError::new(ErrorCode::InvalidRequest, "admission-only fixture"))
        }
    }
    for limit in [0, super::super::NATIVE_CPU_POOL_BYTES + 1] {
        assert!(ImageHost::with_executor(Arc::new(Envelope(limit))).is_err());
    }
    let host = ImageHost::with_executor(Arc::new(Envelope(1024 * 1024 * 1024))).unwrap();
    assert_eq!(host.pool_limit_bytes(), 1024 * 1024 * 1024);
    assert!(host.admit(&empty(), &request(), JobOptions::default()).is_ok());
    assert_eq!(host.admitted_bytes(), 0, "preflight does not allocate or reserve");
    let lease = host.0.shared.budget.reserve(600 * 1024 * 1024).unwrap();
    assert!(host.0.shared.budget.reserve(600 * 1024 * 1024).is_err());
    drop(lease);
    assert_eq!(host.admitted_bytes(), 0);
}

mod partial_preview_contract {
    use super::*;
    use crate::job::PartialPreviewPlan;

    fn credits() -> (Arc<Budget>, Reservation, Arc<PartialCredits>) {
        let plan = PartialPreviewPlan::new(4, 1).unwrap();
        let total = 2 * plan.slot_bytes().unwrap() + 1024 + 17;
        let budget = Arc::new(Budget { used: AtomicU64::new(0), limit: total });
        let mut work = budget.reserve(total).unwrap();
        let credits = PartialCredits::new(plan, &mut work).unwrap();
        assert_eq!(work.bytes, 17);
        (budget, work, credits)
    }
    fn take(credits: &Arc<PartialCredits>) -> ReturnReservation {
        assert!(credits.request().unwrap());
        credits.acquire().unwrap().unwrap()
    }
    fn payload(mut lease: ReturnReservation, sequence: u64) -> Arc<RenderPartialPreview> {
        lease.published().unwrap();
        Arc::new(RenderPartialPreview {
            info: PartialPreviewInfo {
                frame: FrameInfo {
                    job: id(7724, 1),
                    scene_generation: 19,
                    backend: crate::builtin_backend().id,
                    executor: ExecutorId::new("cpu-raytrace").unwrap(),
                    width: 4,
                    height: 1,
                    encoding: ColourEncoding::Srgb,
                    alpha: AlphaConvention::Straight,
                    png_bytes: 4,
                    png_sha256: "unit-lease-fixture".into(),
                },
                sequence,
                source_width: 4,
                source_height: 1,
                completed_passes: 0,
                updated_prefix: 1,
                projection: PartialPreviewProjection::TopLeftRepresentativeV1,
            },
            png: vec![1, 2, 3, 4],
            covered: vec![1, 0, 0, 0],
            _lease: lease,
        })
    }
    #[test]
    fn plan_is_canonical_no_upscale_and_maps_top_left_without_source_scan() {
        for (w, h, expected) in [(16384, 4096, (1024, 256)), (2160, 3840, (576, 1024)), (4, 1, (4, 1)), (u32::MAX, 1, (1024, 1))] {
            let plan = PartialPreviewPlan::new(w, h).unwrap();
            assert_eq!((plan.width(), plan.height()), expected);
            assert_eq!((plan.source_width(), plan.source_height()), (w, h));
            assert_eq!(plan.source_index(0).unwrap(), 0);
            assert!(plan.source_index(plan.pixels() as usize).is_err());
            assert!(plan.pixels() <= 1024 * 1024);
        }
        let plan = PartialPreviewPlan::new(16384, 4096).unwrap();
        assert_eq!(plan.source_index(1025).unwrap(), 16 * 16384 + 16);
        assert_eq!(plan.snapshot_work().unwrap(), 12 * 1024 * 256 + 65538);
        assert!(PartialPreviewPlan::new(0, 1).is_err());
        assert!(PartialPreviewPlan::new(1, 0).is_err());
    }
    #[test]
    fn same_budget_merge_moves_ownership_once_and_wrong_pool_preserves_both() {
        let a = Arc::new(Budget { used: AtomicU64::new(0), limit: 100 });
        let b = Arc::new(Budget { used: AtomicU64::new(0), limit: 100 });
        let mut retained = a.reserve(80).unwrap();
        let mut donor = retained.split(30).unwrap();
        let mut foreign = b.reserve(9).unwrap();
        assert_eq!(retained.merge_from(&mut foreign).unwrap_err().code, ErrorCode::WrongOwner);
        assert_eq!((retained.bytes, foreign.bytes), (50, 9));
        retained.merge_from(&mut donor).unwrap();
        assert_eq!((retained.bytes, donor.bytes, a.used.load(Ordering::Acquire)), (80, 0, 80));
        assert!(retained.split(81).is_err());
        assert_eq!(retained.bytes, 80);
        drop(donor);
        assert_eq!(a.used.load(Ordering::Acquire), 80);
        drop(retained);
        drop(foreign);
        assert_eq!((a.used.load(Ordering::Acquire), b.used.load(Ordering::Acquire)), (0, 0));
    }
    #[test]
    fn two_total_slots_include_inflight_and_clones_do_not_mint_capacity() {
        let (budget, work, credits) = credits();
        let total = budget.used.load(Ordering::Acquire);
        let first = take(&credits);
        assert!(credits.request().unwrap());
        assert!(credits.acquire().unwrap().is_none(), "one encoder even with another free slot");
        let first = payload(first, 1);
        let held = first.clone();
        let second = payload(take(&credits), 2);
        assert!(credits.request().unwrap());
        assert!(credits.acquire().unwrap().is_none());
        assert!(credits.requested.load(Ordering::Acquire), "full credit preserves coalesced demand");
        drop(first);
        assert!(credits.acquire().unwrap().is_none(), "a clone retains the same first slot");
        assert_eq!(budget.used.load(Ordering::Acquire), total);
        drop(held);
        let third = credits.acquire().unwrap().unwrap();
        assert_eq!(budget.used.load(Ordering::Acquire), total, "return/transfer does not reserve globally again");
        assert_eq!(second.png(), &[1, 2, 3, 4]);
        drop(third);
        drop(second);
        credits.close();
        assert_eq!(budget.used.load(Ordering::Acquire), 17 + 1024);
        drop(credits);
        drop(work);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
    #[test]
    fn demand_during_inflight_survives_publication_and_terminal_held_frames_stay_billed() {
        let (budget, work, credits) = credits();
        let first = take(&credits);
        assert!(!credits.requested.load(Ordering::Acquire));
        assert!(credits.request().unwrap());
        let first = payload(first, 1);
        assert!(credits.requested.load(Ordering::Acquire));
        let second = payload(credits.acquire().unwrap().unwrap(), 2);
        let per_slot = credits.plan.slot_bytes().unwrap();
        credits.close();
        assert!(!credits.request().unwrap());
        assert!(credits.acquire().unwrap().is_none());
        drop(work);
        assert_eq!(budget.used.load(Ordering::Acquire), 2 * per_slot + 1024);
        drop(credits);
        assert_eq!(first.covered(), &[1, 0, 0, 0]);
        drop(first);
        assert_eq!(budget.used.load(Ordering::Acquire), per_slot + 1024);
        drop(second);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
    #[test]
    fn optional_exhaustion_is_real_and_legacy_request_remains_noop() {
        let (_, work, credits) = credits();
        assert!(credits.request().unwrap());
        credits.exhaust();
        assert!(credits.exhausted.load(Ordering::Acquire));
        assert!(!credits.request().unwrap());
        assert!(credits.acquire().unwrap().is_none());
        credits.close();
        drop(work);
        let host = CpuHost::cooperative();
        let job = host.submit(id(7724, 2), 19, empty(), request()).unwrap();
        let before = serde_json::to_value(job.status().unwrap()).unwrap();
        assert!(!job.request_partial_preview().unwrap());
        assert!(!job.partial_preview_exhausted().unwrap());
        assert!(job.partial_preview().unwrap().is_none());
        assert_eq!(serde_json::to_value(job.status().unwrap()).unwrap(), before);
        job.cancel().unwrap();
        assert!(!job.request_partial_preview().unwrap());
    }

    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    fn real_fixture() -> (CpuHost, JobHandle, Running) {
        let host = ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }));
        let request = ImageRequest {
            integrator: Integrator::Raytrace,
            raytrace: Some(crate::job::RaytraceOptions {
                progressive: Some(crate::job::CpuProgressiveOptions { samples_per_pixel: 5, seed: 123 }),
                lights: Some(Vec::new()),
                ..Default::default()
            }),
            width: 16,
            height: 16,
            ..request()
        };
        let job = host.submit_cpu_partial_with_environment_and_options(id(7724, 3), 19, empty(), request, JobOptions::default(), None).unwrap();
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let running = Running::begin(work).unwrap();
        (host, job, running)
    }
    #[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
    #[test]
    fn real_partial_png_carries_unknown_and_two_held_payloads_do_not_block_final() {
        let (host, job, mut running) = real_fixture();
        assert!(job.request_partial_preview().unwrap());
        let mut during = false;
        for _ in 0..100_000 {
            assert!(running.advance(1).unwrap());
            if !during && running.partial.as_ref().unwrap().0[0].lease.is_some() {
                assert!(job.request_partial_preview().unwrap());
                during = true;
            }
            if job.partial_preview().unwrap().is_some() {
                break;
            }
        }
        let first = job.partial_preview().unwrap().unwrap();
        assert!(during);
        assert_eq!(first.info.sequence, 1);
        assert_eq!(first.info.completed_passes, 0);
        assert!(first.info.updated_prefix > 0 && first.info.updated_prefix < 256);
        assert!(job.preview().unwrap().is_none(), "partial mode never publishes uncredited legacy snapshots");
        let decoded = image::load_from_memory(first.png()).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (16, 16));
        for (i, rgba) in decoded.pixels().enumerate() {
            assert_eq!(rgba.0[3], if (i as u64) < first.info.updated_prefix { 255 } else { 0 });
            assert_eq!(first.covered()[i], 0, "empty scene means a real miss, never a fabricated hit");
        }
        let first_hash = Sha256::digest(first.png());
        for _ in 0..100_000 {
            if job.partial_preview().unwrap().is_some_and(|p| p.info.sequence == 2) {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        let second = job.partial_preview().unwrap().unwrap();
        assert_eq!(second.info.sequence, 2, "request issued during encoding must survive first publication");
        assert!(job.request_partial_preview().unwrap());
        assert_eq!(job.0.partial.as_ref().unwrap().state.lock().unwrap().free.bytes, 0);
        for _ in 0..100_000 {
            if !running.advance(1).unwrap() {
                break;
            }
            assert!(job.partial_preview().unwrap().is_some_and(|p| p.info.sequence == 2));
            assert!(running.partial.as_ref().unwrap().0[0].lease.is_none());
            assert!(running.partial.as_ref().unwrap().0[0].pending.is_none());
        }
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        let final_frame = job.frame().unwrap().unwrap();
        assert_eq!((final_frame.info.width, final_frame.info.height), (16, 16));
        assert!(job.partial_preview().unwrap().is_none());
        assert!(!job.request_partial_preview().unwrap());
        assert_eq!(Sha256::digest(first.png()), first_hash);
        let per_slot = job.0.partial.as_ref().unwrap().plan.slot_bytes().unwrap();
        drop(running);
        let before = host.admitted_bytes();
        drop(first);
        assert_eq!(host.admitted_bytes(), before - per_slot);
        drop(second);
        assert_eq!(host.admitted_bytes(), before - 2 * per_slot);
        drop(final_frame);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod partial_preview_terminal_errors {
    use super::*;
    use crate::job::{CpuProgressiveOptions, RaytraceOptions};

    #[test]
    fn cancellation_during_real_partial_hash_closes_slots_and_publishes_nothing() {
        let host = ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }));
        let request = ImageRequest {
            integrator: Integrator::Raytrace,
            raytrace: Some(RaytraceOptions {
                progressive: Some(CpuProgressiveOptions { samples_per_pixel: 5, seed: 7 }),
                lights: Some(Vec::new()),
                ..Default::default()
            }),
            width: 8,
            height: 8,
            ..request()
        };
        let job = host.submit_cpu_partial_with_environment_and_options(id(7725, 1), 19, empty(), request, JobOptions::default(), None).unwrap();
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let mut running = Running::begin(work).unwrap();
        assert!(job.request_partial_preview().unwrap());
        for _ in 0..100_000 {
            if running.pending_png.is_some() {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        assert!(running.pending_png.is_some());
        assert!(job.partial_preview().unwrap().is_none());
        assert!(running.partial.as_ref().unwrap().0[0].pending.is_some());
        job.cancel().unwrap();
        assert!(!job.request_partial_preview().unwrap());
        assert!(!running.guarded_step(1));
        assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(job.partial_preview().unwrap().is_none());
        assert!(job.frame().unwrap().is_none());
        drop(running);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

#[test]
fn partial_credit_last_drop_fails_closed_after_poison_without_losing_leases() {
    let plan = crate::job::PartialPreviewPlan::new(1, 1).unwrap();
    let bytes = 2 * plan.slot_bytes().unwrap() + 1024;
    let budget = Arc::new(Budget { used: AtomicU64::new(0), limit: bytes });
    let mut work = budget.reserve(bytes).unwrap();
    let credits = PartialCredits::new(plan, &mut work).unwrap();
    credits.request().unwrap();
    let held = credits.acquire().unwrap().unwrap();
    let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _state = credits.state.lock().unwrap();
        panic!("deliberate test-only credit lock poison");
    }));
    assert!(poisoned.is_err());
    drop(held);
    assert_eq!(budget.used.load(Ordering::Acquire), 1024);
    credits.close();
    drop(credits);
    drop(work);
    assert_eq!(budget.used.load(Ordering::Acquire), 0);
}

mod protected_feedback_contract {
    use super::*;
    use crate::job::{CpuBudget, MAX_CPU_WORK, MAX_PROTECTED_CPU_WORK, ProtectedPreviewPlan, ProtectedPreviewReport};

    #[test]
    fn profile_ordinary_caps_are_unchanged_and_protected_aggregate_is_explicit() {
        assert_eq!(MAX_CPU_WORK, 16_000_000_000_000);
        assert_eq!(MAX_PROTECTED_CPU_WORK, 16_000_809_500_800);
        for (profile, ordinary) in
            [(CpuBudget::Compact, 1_000_000_000), (CpuBudget::Studio, 1_000_000_000_000), (CpuBudget::Workstation, MAX_CPU_WORK)]
        {
            let plan = ProtectedPreviewPlan::new(1024, 1024, 16, profile).unwrap();
            plan.validate().unwrap();
            assert_eq!(plan.ordinary_limit(), ordinary);
            assert_eq!(plan.quota_limit(), 64);
            assert_eq!(plan.snapshot_work().unwrap(), 12_648_450);
            assert_eq!(plan.presentation_limit().unwrap(), 809_500_800);
            assert_eq!(plan.aggregate_limit().unwrap(), ordinary + 809_500_800);
            assert_eq!(plan.preview_plan(), crate::job::PartialPreviewPlan::new(1024, 1024).unwrap());
        }
        let largest = ProtectedPreviewPlan::new(1024, 1024, 16, CpuBudget::Workstation).unwrap();
        assert_eq!(largest.aggregate_limit().unwrap(), MAX_PROTECTED_CPU_WORK);
    }
    #[test]
    fn zero_single_and_spread_milestones_are_checked_below_final_target() {
        let zero = ProtectedPreviewPlan::new(1, 1, 1, CpuBudget::Compact).unwrap();
        assert_eq!((zero.total_commits(), zero.quota_limit(), zero.presentation_limit().unwrap()), (1, 0, 0));
        assert_eq!(zero.aggregate_limit().unwrap(), zero.ordinary_limit());
        assert!(zero.milestone(0).is_err());
        let single = ProtectedPreviewPlan::new(2, 1, 1, CpuBudget::Compact).unwrap();
        assert_eq!(single.quota_limit(), 1);
        assert_eq!(single.milestone(0).unwrap(), 1);
        assert!(single.milestone(1).is_err());
        for (w, h, spp, quota) in [(3, 1, 1, 2), (66, 1, 1, 64), (32, 32, 64, 64)] {
            let plan = ProtectedPreviewPlan::new(w, h, spp, CpuBudget::Compact).unwrap();
            assert_eq!(plan.quota_limit(), quota);
            let milestones: Vec<_> = (0..quota).map(|j| plan.milestone(j).unwrap()).collect();
            assert_eq!(milestones.first(), Some(&1));
            assert_eq!(milestones.last(), Some(&(plan.total_commits() - 1)));
            assert!(milestones.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(plan.milestone(quota).is_err());
            assert!(plan.milestone(u32::MAX).is_err());
        }
    }
    #[test]
    fn protected_plan_preserves_explicit_large_profiles_and_rejects_invalid_inputs() {
        let large = ProtectedPreviewPlan::new(4096, 4096, 4096, CpuBudget::Workstation).unwrap();
        assert_eq!(large.samples_per_pixel(), 4096);
        assert_eq!(large.budget(), CpuBudget::Workstation);
        assert_eq!(large.total_commits(), 68_719_476_736);
        assert_eq!((large.preview_plan().width(), large.preview_plan().height()), (1024, 1024));
        assert!(large.aggregate_limit().unwrap() <= MAX_PROTECTED_CPU_WORK);
        for (w, h, spp, budget) in [
            (0, 1, 1, CpuBudget::Compact),
            (1, 0, 1, CpuBudget::Compact),
            (1, 1, 0, CpuBudget::Compact),
            (1, 1, 65, CpuBudget::Compact),
            (u32::MAX, u32::MAX, u32::MAX, CpuBudget::Workstation),
            (4096, 4096, 4096, CpuBudget::Compact),
        ] {
            assert!(ProtectedPreviewPlan::new(w, h, spp, budget).is_err());
        }
    }

    // Synthetic REPORT-validation fixtures only; real producer usage is tested below.
    fn report(plan: ProtectedPreviewPlan, ordinary_used: u64, accepted: u32, next: u32) -> ProtectedPreviewReport {
        ProtectedPreviewReport {
            policy_version: 1,
            ordinary_limit: plan.ordinary_limit(),
            presentation_limit: plan.presentation_limit().unwrap(),
            aggregate_limit: plan.aggregate_limit().unwrap(),
            ordinary_used,
            presentation_used: u64::from(accepted) * plan.snapshot_work().unwrap(),
            accepted_snapshots: accepted,
            quota_limit: plan.quota_limit(),
            next_milestone_index: next,
            quota_exhausted: accepted == plan.quota_limit(),
            milestones_complete: next == plan.quota_limit(),
        }
    }
    fn credits(plan: ProtectedPreviewPlan) -> (Arc<Budget>, Reservation, Arc<PartialCredits>) {
        let total = 2 * plan.preview_plan().slot_bytes().unwrap() + 1024;
        let budget = Arc::new(Budget { used: AtomicU64::new(0), limit: total });
        let mut work = budget.reserve(total).unwrap();
        let credits = PartialCredits::new_protected(plan, &mut work).unwrap();
        assert_eq!(work.bytes, 0);
        (budget, work, credits)
    }
    #[test]
    fn coalesced_milestones_do_not_fabricate_spent_quota_or_permanent_exhaustion() {
        let plan = ProtectedPreviewPlan::new(32, 32, 64, CpuBudget::Compact).unwrap();
        let (budget, work, credits) = credits(plan);
        assert!(credits.state.lock().unwrap().report.is_none());
        let value = report(plan, 31, 1, 64);
        credits.store_report(Some(value)).unwrap();
        assert_eq!(credits.state.lock().unwrap().report, Some(value));
        assert!(value.milestones_complete && !value.quota_exhausted);
        assert_eq!(value.presentation_used, 77_826);
        assert!(value.presentation_used < value.presentation_limit);
        assert!(!credits.exhausted.load(Ordering::Acquire));
        assert!(credits.request().unwrap());
        assert!(credits.state.lock().unwrap().report == Some(value));
        credits.close();
        drop(credits);
        drop(work);
        assert_eq!(budget.used.load(Ordering::Acquire), 0);
    }
    #[test]
    fn actual_report_invariants_and_monotonic_refusals_preserve_the_prior_observation() {
        let plan = ProtectedPreviewPlan::new(32, 32, 64, CpuBudget::Compact).unwrap();
        let (_, work, credits) = credits(plan);
        let good = report(plan, 31, 1, 2);
        credits.store_report(Some(good)).unwrap();
        let bad = [
            report(plan, 30, 1, 2),
            report(plan, 31, 0, 2),
            report(plan, 31, 1, 1),
            ProtectedPreviewReport { ordinary_used: plan.ordinary_limit() + 1, ..good },
            ProtectedPreviewReport { presentation_used: plan.presentation_limit().unwrap() + 1, ..good },
            ProtectedPreviewReport { aggregate_limit: MAX_PROTECTED_CPU_WORK + 1, ..good },
            ProtectedPreviewReport { quota_exhausted: true, ..good },
            ProtectedPreviewReport { milestones_complete: true, ..good },
            ProtectedPreviewReport { policy_version: 2, ..good },
        ];
        for value in bad {
            assert!(credits.store_report(Some(value)).is_err());
            assert_eq!(credits.state.lock().unwrap().report, Some(good));
        }
        assert!(credits.store_report(None).is_err());
        assert_eq!(credits.state.lock().unwrap().report, Some(good));
        let at_limits = report(plan, plan.ordinary_limit(), plan.quota_limit(), plan.quota_limit());
        credits.store_report(Some(at_limits)).unwrap();
        assert_eq!(at_limits.ordinary_used + at_limits.presentation_used, at_limits.aggregate_limit);
        assert!(credits.exhausted.load(Ordering::Acquire));
        assert!(!credits.request().unwrap());
        credits.close();
        drop(credits);
        drop(work);
    }
    #[test]
    fn legacy_report_is_absent_and_rejected_protected_admission_does_not_reserve() {
        let host = CpuHost::cooperative();
        let job = host.submit(id(7730, 1), 19, empty(), request()).unwrap();
        let before = serde_json::to_value(job.status().unwrap()).unwrap();
        assert!(job.protected_preview_report().unwrap().is_none());
        assert_eq!(serde_json::to_value(job.status().unwrap()).unwrap(), before);
        let admitted = host.admitted_bytes();
        assert!(
            host.submit_cpu_protected_partial_with_environment_and_options(id(7730, 2), 19, empty(), request(), JobOptions::default(), None).is_err()
        );
        assert_eq!(host.admitted_bytes(), admitted);
        assert!(job.protected_preview_report().unwrap().is_none());
    }
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod protected_feedback_real_host {
    use super::*;
    use crate::job::{CpuProgressiveOptions, RaytraceOptions};

    fn fixture(serial: u64, width: u32, height: u32, samples: u32) -> (CpuHost, JobHandle, Running) {
        let host = ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }));
        let request = ImageRequest {
            integrator: Integrator::Raytrace,
            raytrace: Some(RaytraceOptions {
                progressive: Some(CpuProgressiveOptions { samples_per_pixel: samples, seed: 123 }),
                lights: Some(Vec::new()),
                ..Default::default()
            }),
            width,
            height,
            background: [10, 20, 30],
            ..request()
        };
        let job = host
            .submit_cpu_protected_partial_with_environment_and_options(id(7731, serial), 19, empty(), request, JobOptions::default(), None)
            .unwrap();
        assert!(job.protected_preview_report().unwrap().is_none(), "no future ledger is invented while queued");
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        let running = Running::begin(work).unwrap();
        assert!(job.protected_preview_report().unwrap().is_some(), "report is read from the real constructed task");
        (host, job, running)
    }
    fn finish(running: &mut Running) {
        for _ in 0..1_000_000 {
            if !running.advance(1).unwrap() {
                return;
            }
        }
        panic!("small protected CPU fixture did not finish");
    }
    #[test]
    fn actual_32_by_32_control_and_feedback_keep_ordinary_work_and_final_png_identical() {
        let (control_host, control, mut control_run) = fixture(1, 32, 32, 64);
        finish(&mut control_run);
        let control_report = control.protected_preview_report().unwrap().unwrap();
        let control_frame = control.frame().unwrap().unwrap();
        assert_eq!(control_report.presentation_used, 0);
        assert_eq!(control_report.accepted_snapshots, 0);
        assert!(control_report.ordinary_used > 0);
        let (host, job, mut running) = fixture(2, 32, 32, 64);
        assert!(job.request_partial_preview().unwrap());
        finish(&mut running);
        let report = job.protected_preview_report().unwrap().unwrap();
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        assert_eq!(report.ordinary_used, control_report.ordinary_used);
        assert_eq!(report.ordinary_limit, 1_000_000_000);
        assert_eq!(report.accepted_snapshots, 1);
        assert_eq!(report.presentation_used, 77_826);
        assert_eq!(report.ordinary_used + report.presentation_used, control_report.ordinary_used + 77_826);
        assert_eq!(report.aggregate_limit, report.ordinary_limit + report.presentation_limit);
        assert!(!report.quota_exhausted);
        let frame = job.frame().unwrap().unwrap();
        assert_eq!(frame.png(), control_frame.png());
        let decoded = image::load_from_memory(frame.png()).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (32, 32));
        assert!(decoded.pixels().all(|p| p.0 == [10, 20, 30, 255]), "analytic all-miss final pixels");
        let terminal = job.protected_preview_report().unwrap();
        assert!(!job.request_partial_preview().unwrap());
        assert_eq!(job.protected_preview_report().unwrap(), terminal);
        drop(frame);
        drop(job);
        drop(running);
        drop(control_frame);
        drop(control);
        drop(control_run);
        assert_eq!((host.admitted_bytes(), control_host.admitted_bytes()), (0, 0));
    }
    #[test]
    fn real_quota_exhaustion_does_not_remove_normal_full_pass_or_final_work() {
        let (control_host, control, mut control_run) = fixture(3, 2, 1, 2);
        finish(&mut control_run);
        let expected = control.protected_preview_report().unwrap().unwrap();
        let control_frame = control.frame().unwrap().unwrap();
        let (host, job, mut running) = fixture(4, 2, 1, 2);
        let mut exhausted_seen = false;
        for _ in 0..100_000 {
            job.request_partial_preview().unwrap();
            let more = running.advance(1).unwrap();
            let report = job.protected_preview_report().unwrap().unwrap();
            if report.quota_exhausted {
                exhausted_seen = true;
                assert!(job.partial_preview_exhausted().unwrap());
                assert!(!job.request_partial_preview().unwrap());
            }
            if !more {
                break;
            }
        }
        assert!(exhausted_seen);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        let report = job.protected_preview_report().unwrap().unwrap();
        assert_eq!((report.accepted_snapshots, report.quota_limit, report.next_milestone_index), (3, 3, 3));
        assert!(report.quota_exhausted && report.milestones_complete);
        assert_eq!(report.presentation_used, report.presentation_limit);
        assert_eq!(report.ordinary_used, expected.ordinary_used, "full-pass tick and final remain ordinary");
        let frame = job.frame().unwrap().unwrap();
        assert_eq!(frame.png(), control_frame.png());
        drop(frame);
        drop(job);
        drop(running);
        drop(control_frame);
        drop(control);
        drop(control_run);
        assert_eq!((host.admitted_bytes(), control_host.admitted_bytes()), (0, 0));
    }
    #[test]
    fn two_retained_protected_payloads_keep_their_leases_without_blocking_final() {
        let (host, job, mut running) = fixture(5, 2, 1, 2);
        assert!(job.request_partial_preview().unwrap());
        for _ in 0..100_000 {
            if job.partial_preview().unwrap().is_some() {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        let first = job.partial_preview().unwrap().unwrap();
        assert!(job.request_partial_preview().unwrap());
        for _ in 0..100_000 {
            if job.partial_preview().unwrap().is_some_and(|p| p.info.sequence == 2) {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        let second = job.partial_preview().unwrap().unwrap();
        assert_eq!(second.info.sequence, 2);
        let old_hash = Sha256::digest(first.png());
        assert!(job.request_partial_preview().unwrap());
        assert_eq!(job.0.partial.as_ref().unwrap().state.lock().unwrap().free.bytes, 0);
        finish(&mut running);
        assert_eq!(job.status().unwrap().progress.phase, Phase::Completed);
        let report = job.protected_preview_report().unwrap().unwrap();
        assert_eq!((report.accepted_snapshots, report.quota_limit), (2, 3));
        assert!(!report.quota_exhausted);
        assert_eq!(Sha256::digest(first.png()), old_hash);
        drop(running);
        let slot = job.0.partial.as_ref().unwrap().plan.slot_bytes().unwrap();
        let retained = host.admitted_bytes();
        drop(first);
        assert_eq!(host.admitted_bytes(), retained - slot);
        drop(second);
        assert_eq!(host.admitted_bytes(), retained - 2 * slot);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
    #[test]
    fn charged_begin_followed_by_poll_error_retains_actual_report_without_refund() {
        let (host, job, mut running) = fixture(6, 32, 32, 64);
        let plan = job.0.partial.as_ref().unwrap().plan;
        for _ in 0..100_000 {
            if running.task.as_mut().unwrap().partial_ready(plan).unwrap() {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        assert!(running.task.as_mut().unwrap().partial_ready(plan).unwrap());
        assert!(job.request_partial_preview().unwrap());
        // Invalid zero quantum fails AFTER the host's accepted safe BEGIN/precharge.
        assert!(!running.guarded_step(0));
        assert_eq!(job.status().unwrap().progress.phase, Phase::Failed);
        let report = job.protected_preview_report().unwrap().unwrap();
        assert_eq!(report.accepted_snapshots, 1);
        assert_eq!(report.presentation_used, 77_826);
        assert!(job.frame().unwrap().is_none());
        assert!(!job.request_partial_preview().unwrap());
        drop(running);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
    #[test]
    fn cancellation_while_host_hashes_does_not_charge_twice_or_refund_feedback() {
        let (host, job, mut running) = fixture(7, 32, 32, 64);
        assert!(job.request_partial_preview().unwrap());
        for _ in 0..100_000 {
            if running.pending_png.is_some() {
                break;
            }
            assert!(running.advance(1).unwrap());
        }
        assert!(running.pending_png.is_some());
        let before = job.protected_preview_report().unwrap().unwrap();
        assert_eq!((before.accepted_snapshots, before.presentation_used), (1, 77_826));
        job.cancel().unwrap();
        assert!(!running.guarded_step(1));
        assert_eq!(job.protected_preview_report().unwrap(), Some(before));
        assert_eq!(job.status().unwrap().progress.phase, Phase::Cancelled);
        assert!(job.frame().unwrap().is_none());
        drop(running);
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod protected_begin_setup_errors {
    use super::*;
    use crate::job::{CpuProgressiveOptions, EmissionSides, RaytraceOptions, SurfaceEmission};
    use solidcraft_geom::Vec3;

    fn fixture(serial: u64) -> (CpuHost, JobHandle, Work) {
        let host = ImageHost(Arc::new(Owner {
            executor: None,
            shared: CpuHost::shared(MEMORY_LIMIT),
            kind: HostKind::NativeWorker,
            running: Mutex::new(None),
            _worker: None,
        }));
        let mesh = Arc::new(Mesh {
            positions: vec![Vec3::ZERO, Vec3::X, Vec3::Y],
            normals: vec![Vec3::Z; 3],
            triangles: vec![[0, 1, 2]],
            tri_face: vec![0],
            ..Default::default()
        });
        let scene = ProductScene {
            bounds: mesh.bounds(),
            meshes: vec![crate::ProductMesh {
                name: "Setup witness".into(),
                mesh,
                color: [0; 3],
                opacity: 1.0,
                surface: None,
                transmission: None,
                faces: Vec::new(),
            }],
        };
        let request = ImageRequest {
            integrator: Integrator::Raytrace,
            raytrace: Some(RaytraceOptions {
                progressive: Some(CpuProgressiveOptions { samples_per_pixel: 2, seed: 123 }),
                lights: Some(Vec::new()),
                emissions: Some(vec![SurfaceEmission {
                    body_index: 0,
                    body: "Setup witness".into(),
                    color: [1.0, 0.0, 0.0],
                    strength: 2.0,
                    sides: EmissionSides::Both,
                }]),
                ..Default::default()
            }),
            width: 2,
            height: 1,
            ..request()
        };
        assert_eq!(request.raytrace.as_ref().unwrap().emissions.as_ref().unwrap().len(), 1);
        let job = host
            .submit_cpu_protected_partial_with_environment_and_options(id(7732, serial), 19, scene, request, JobOptions::default(), None)
            .unwrap();
        assert!(job.protected_preview_report().unwrap().is_none());
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        (host, job, work)
    }
    fn observed_report(credits: &PartialCredits) -> crate::job::ProtectedPreviewReport {
        let Ok(state) = credits.state.try_lock() else { panic!("wrapping factory must run outside the report/credit lock") };
        let actual = state.report.expect("real task report must be captured BEFORE fallible wrapping");
        assert_eq!(actual.policy_version, 1);
        assert_eq!(actual.ordinary_limit, 1_000_000_000);
        assert_eq!(actual.accepted_snapshots, 0);
        assert_eq!(actual.ordinary_used, 1, "the single emission retains its pre-existing ordinary constructor tick");
        assert_eq!(actual.presentation_used, 0);
        actual
    }
    fn assert_failed_report(host: CpuHost, job: JobHandle, actual: crate::job::ProtectedPreviewReport, message: &str) {
        assert_eq!(job.protected_preview_report().unwrap(), Some(actual), "no invented zeros or discarded real constructor work");
        let status = job.status().unwrap();
        assert_eq!(status.progress.phase, Phase::Failed);
        let error = status.error.unwrap();
        assert_eq!(error.code, ErrorCode::AllocationFailed);
        assert_eq!(error.message, message);
        assert!(job.frame().unwrap().is_none());
        assert!(job.partial_preview().unwrap().is_none());
        assert!(!job.request_partial_preview().unwrap());
        let request_bytes = job.0._request_reservation.as_ref().map_or(0, |lease| lease.bytes);
        assert_eq!(host.admitted_bytes(), request_bytes + 1024, "task/free slots released; actual report metadata retained");
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
    #[test]
    fn protected_setup_failure_preserves_constructed_task_report_and_error() {
        let (host, job, work) = fixture(1);
        let observed = std::cell::Cell::new(None);
        let running = Running::begin_with_partial_factory(work, |credits| {
            // Deterministic fault AFTER the REAL producer and first host slot exist.
            observed.set(Some(observed_report(&credits)));
            Err(JobError::new(ErrorCode::AllocationFailed, "injected auxiliary setup failure"))
        });
        assert!(running.is_none());
        let actual = observed.get().expect("post-construction factory must be reached");
        assert_failed_report(host, job, actual, "injected auxiliary setup failure");
    }
    #[test]
    fn first_host_slot_failure_preserves_the_raw_producer_report() {
        let (host, job, work) = fixture(2);
        let Work { scene, handle, reservation, environment, .. } = work;
        let plan = handle.0.partial.as_ref().unwrap().protected.unwrap();
        let options = handle.0.request.raytrace.clone().unwrap();
        let task = crate::raytrace::image::ImageTask::new_with_environment_and_nested_and_protected_partial(
            scene,
            handle.0.request.clone(),
            handle.0.cancel.clone(),
            options,
            environment,
            handle.0.options.nested_media,
            plan,
        )
        .unwrap();
        let observed = std::cell::Cell::new(None);
        let result = WorkTask::raytrace_with_slot(task, handle.0.partial.as_deref(), |_task| {
            // The actual producer exists even though its FIRST wrapper cannot be made.
            observed.set(Some(observed_report(handle.0.partial.as_deref().unwrap())));
            Err(JobError::new(ErrorCode::AllocationFailed, "injected first host slot failure"))
        });
        let error = result.err().expect("injected wrapping failure must propagate");
        let actual = observed.get().expect("first-slot factory must be reached");
        terminal(&handle, error);
        drop(reservation);
        drop(handle);
        assert_failed_report(host, job, actual, "injected first host slot failure");
    }
    #[test]
    fn an_actual_constructor_refusal_keeps_report_none_and_skips_auxiliary_setup() {
        let (host, job, mut work) = fixture(3);
        drop(job);
        // Private invalid-constructor fixture only; no public admission/cap bypass.
        Arc::get_mut(&mut work.handle.0).unwrap().request.width = 0;
        let job = work.handle.clone();
        let running = Running::begin_with_partial_factory(work, |_| panic!("constructor refusal must not attempt auxiliary setup"));
        assert!(running.is_none());
        assert!(job.protected_preview_report().unwrap().is_none());
        assert_eq!(job.status().unwrap().progress.phase, Phase::Failed);
        assert!(job.frame().unwrap().is_none());
        drop(job);
        assert_eq!(host.admitted_bytes(), 0);
    }
}

#[cfg(all(feature = "raytrace-cpu", not(target_arch = "wasm32")))]
mod directional_shadow_contract {
    use super::*;

    // Same queue-only native metadata harness as existing protected setup tests.
    // No native worker is spawned; drive the real Running::begin deterministically.
    // This checks host queues/leases, not native scheduling qualification.
    fn queued_native_host() -> CpuHost {
        ImageHost(Arc::new(Owner {
            executor: None, shared: CpuHost::shared(MEMORY_LIMIT), kind: HostKind::NativeWorker,
            running: Mutex::new(None), _worker: None,
        }))
    }

    fn capture() -> (ProductScene, ImageRequest) {
        let mesh = Mesh {
            positions: vec![solidcraft_geom::Vec3::new(-10.0,-10.0,0.0),solidcraft_geom::Vec3::new(10.0,-10.0,0.0),solidcraft_geom::Vec3::new(10.0,10.0,0.0),solidcraft_geom::Vec3::new(-10.0,10.0,0.0)],
            triangles: vec![[0,1,2],[0,2,3]], ..Default::default()
        };
        let bounds = mesh.bounds();
        let scene = ProductScene { bounds, meshes: vec![crate::ProductMesh {
            name: "directional glass".into(), mesh: std::sync::Arc::new(mesh), color:[255;3], opacity:1.0,
            surface:None, faces:Vec::new(), transmission:None,
        }] };
        let mut input = request();
        input.integrator = Integrator::Raytrace;
        input.raytrace = Some(super::super::super::RaytraceOptions {
            directional_shadow_transmission:true,
            dielectrics:vec![super::super::super::DielectricOptions {
                body_index:0, body_name:"directional glass".into(), boundary:super::super::super::DielectricBoundary::ThinSheet,
                ior:1.0, absorption_per_mm:[0.0;3], thickness_mm:1.0,
            }], ..Default::default()
        });
        (scene,input)
    }

    #[test]
    fn external_hdr_is_refused_before_admission_or_retained_leases() {
        use crate::lighting::{HdrSource, environment_input::EnvironmentInput};
        let mut bytes = b"#?RADIANCE\nFORMAT=32-bit_rle_rgbe\nPRIMARIES=0.64 0.33 0.30 0.60 0.15 0.06 0.3127 0.3290\n\n-Y 2 +X 4\n".to_vec();
        for _ in 0..8 { bytes.extend_from_slice(&[128,128,128,129]); }
        let environment = EnvironmentInput::new(HdrSource::new(bytes).unwrap(),0.0,1.0,4).unwrap();
        let host = queued_native_host();
        let (scene,input) = capture();
        let error = host.admit_submission(&scene,&input,JobOptions::default(),Some(&environment),None).err().unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(error.message, "directional shadow transmission does not support HDR environments");
        let error = host.submit_with_environment(id(9620,1),1,scene,input,Some(environment)).err().unwrap();
        assert_eq!(error.code, ErrorCode::InvalidRequest);
        assert_eq!(error.message, "directional shadow transmission does not support HDR environments");
        assert_eq!(host.admitted_bytes(),0);
        assert!(host.0.shared.queue.lock().unwrap().is_empty());
    }

    #[test]
    fn cancelled_directional_request_publishes_no_frame_and_releases_work() {
        let host = queued_native_host();
        let (scene,input) = capture();
        let retained = input.raytrace.as_ref().unwrap().retained_bytes().unwrap();
        let handle = host.submit(id(9621,1),1,scene,input).unwrap();
        assert!(handle.status().unwrap().request.raytrace.as_ref().unwrap().directional_shadow_transmission);
        handle.cancel().unwrap();
        let work = host.0.shared.queue.lock().unwrap().pop_front().unwrap();
        assert!(Running::begin(work).is_none());
        assert_eq!(handle.status().unwrap().progress.phase,Phase::Cancelled);
        assert!(handle.frame().unwrap().is_none());
        assert_eq!(host.admitted_bytes(),retained);
        drop(handle);
        assert_eq!(host.admitted_bytes(),0);
    }
}

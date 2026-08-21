## Resumen ejecutivo
- Riesgo alto: 3 (line_cap OOM, spawn leak, cancel bloqueado 90s)
- Riesgo medio: 6 (timeouts hardcodeados, validate rama muerta, shutdown consume events, diagnostics unwrap, recv_event ambiguo, wait_ready silencia Error)
- Propuesta: Statem AnimJobState con transiciones tipadas

## 1. spawn (L68-107)
- S1 Alta: Leak Child si take() falla -> Err sin kill.
- S2 Media: No valida working_dir.
- S3 Baja: NUL args no validados.
- S4 Media: Canal mpsc ilimitado -> bounded 128.
- Fix: kill_child helper en cada early return, validar dir, canal acotado.

## 2. wait_ready (L114-146)
- H1 Media: Timeouts hardcodeados 8s/4s ignoran EngineConfig idle_timeout (assistant usa 2s).
- H2 Media: Ok(_) => {} descarta Error temprano.
- H3 Media: Version mismatch sin shutdown -> leak si API cruda.
- H4 Baja: Polling 1s impreciso -> usar remaining saturating.
- Statem: AwaitingHello -> AwaitingPong -> Ready con deadlines tipados.

## 3. submit (L148-164)
- Solo permitido en Ready, send puede bloquear sin timeout.

## 4. recv_event (L166-177)
- Ok(None) ambiguo (timeout vs ignored Hello/Pong). Debe distinguir.

## 5. shutdown (L179-200)
- SD1: checked_add fallback Instant::now -> 0 timeout si overflow, usar saturating.
- SD2: events.recv_timeout(50ms) consume Results reales.
- SD3: Threads no joineados.

## 6. line_cap_bytes (L324-369)
- LC1 Alta: OOM si linea sin \n y >cap -> acumula sin limite aunque marca oversized.
- LC2: Off-by-one con newline a caballo.
- Fix: drenar sin acumular tras superar cap.

## 7. diagnostics
- D1: lock().unwrap() panic poison -> unwrap_or_else poison-aware.

## 8. validate_media_path (L292-322)
- V1 Alta: rama muerta if is_absolute && !RootDir -> nunca dispara.
- V2 Media: TOCTOU symlink.
- V3 Media: working_dir None -> Path(.) desacoplado.
- Fix: eliminar rama, exigir workdir canonico, NUL check, re-validar.

## 9. run_job (L243-290)
- RJ1 Alta: cancel solo al inicio loop, recv_event bloquea 90s -> usar poll 200ms.
- RJ2 Media: timeout no incluye wait_ready.
- RJ3: filtrar por job_id.

## 10. Statem AnimJobState
Idle -> Spawning -> AwaitingHello -> AwaitingPong -> Ready -> Running{job_id, deadline} -> (Completed/Failed/Cancelling/TimedOut) -> ShuttingDown -> Idle
Guardas: validate_media_path, line_cap, cancel poll.

## 11. Parches P0
- Reescribir spawn_reader con limite estricto.
- spawn: kill on Err.
- wait_ready: usar config.idle_timeout.
- run_job poll 200ms.

## 12. Parches P1
- diagnostics poison-aware.
- validate_media_path reescrito.
- deadline saturating.

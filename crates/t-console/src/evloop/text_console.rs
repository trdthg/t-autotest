use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use anyhow::Result;
use tracing::{debug, error, info};

use crate::{parse_str_from_vt100_bytes, EvLoopCtl, Req, Res};

pub struct BufEvLoopCtl {
    ctl: EvLoopCtl,
    buffer: Vec<u8>,
    last_buffer_start: usize,
}

impl BufEvLoopCtl {
    pub fn new(ctl: EvLoopCtl) -> Self {
        Self {
            ctl,
            buffer: Vec::new(),
            last_buffer_start: 0,
        }
    }

    pub fn history(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    pub fn send(&self, req: Req) -> Result<Res, mpsc::RecvError> {
        self.ctl.send(req)
    }

    pub fn write(&mut self, s: &[u8]) {
        self.ctl.send(Req::Write(s.to_vec())).unwrap();
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        self.ctl.send(Req::Write(s.as_bytes().to_vec())).unwrap();
        Ok(())
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        std::thread::sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!();
        let cmd = format!("{cmd}; echo $?{nanoid}\n",);
        self.write_string(&cmd)?;

        self.comsume_buffer_and_map_inner(timeout, |buffer| {
            // find target pattern from buffer
            let parsed_str = parse_str_from_vt100_bytes(buffer);
            let Ok(catched_output) =
                t_util::assert_capture_between(&parsed_str, &format!("{nanoid}\n"), &nanoid)
            else {
                return ConsumeAction::BreakValue((1, "invalid consume regex".to_string()));
            };
            match catched_output {
                Some(v) => {
                    info!(
                        msg = "catched_output",
                        nanoid = nanoid,
                        parsed_str = parsed_str,
                    );
                    if let Some((res, flag)) = v.rsplit_once('\n') {
                        info!(nanoid = nanoid, flag = flag, res = res);
                        if let Ok(flag) = flag.parse::<i32>() {
                            return ConsumeAction::BreakValue((flag, res.to_string()));
                        }
                    } else {
                        // some command doesn't print, like 'sleep'
                        if let Ok(flag) = v.parse::<i32>() {
                            return ConsumeAction::BreakValue((flag, "".to_string()));
                        }
                    }
                    ConsumeAction::BreakValue((1, v))
                }
                // means continue
                None => {
                    debug!(
                        msg = "current bufferw",
                        nanoid = nanoid,
                        parsed_str = parsed_str
                    );
                    ConsumeAction::Continue
                }
            }
        })
    }

    pub fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T> {
        self.comsume_buffer_and_map_inner(timeout, |bytes| {
            f(bytes).map_or(ConsumeAction::Continue, |v| ConsumeAction::BreakValue(v))
        })
    }

    fn comsume_buffer_and_map_inner<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> ConsumeAction<T>,
    ) -> Result<T> {
        let deadline = Instant::now() + timeout;

        let mut buffer_len = 0;
        loop {
            if Instant::now() > deadline {
                break;
            }
            let res = self.ctl.send(Req::Read);
            match res {
                Ok(Res::Value(ref received)) => {
                    // save to buffer
                    if received.is_empty() {
                        continue;
                    }

                    buffer_len += received.len();
                    self.buffer.extend(received);
                    info!(
                        msg = "event loop",
                        buffer_len = received.len(),
                        history_len = self.buffer.len()
                    );

                    // find target pattern
                    let res = f(&self.buffer[self.last_buffer_start..]);

                    match res {
                        ConsumeAction::BreakValue(v) => {
                            // cut from last find
                            self.last_buffer_start = self.buffer.len() - buffer_len;
                            return Ok(v);
                        }
                        ConsumeAction::Continue => {
                            continue;
                        }
                    }
                }
                Ok(t) => {
                    error!(msg = "invalid msg varient", t = ?t);
                    panic!();
                }
                Err(e) => {
                    panic!("{}", format!("{}", e));
                }
            }
        }
        Err(anyhow::anyhow!("timeout"))
    }
}

enum ConsumeAction<T> {
    BreakValue(T),
    Continue,
}

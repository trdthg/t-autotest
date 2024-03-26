use super::evloop::{EvLoopCtl, Req, Res};
use crate::{term::Term, ConsoleError};
use std::{
    marker::PhantomData,
    thread,
    time::{Duration, Instant},
};
use tracing::{debug, error, info};

type Result<T> = std::result::Result<T, ConsoleError>;

pub struct Tty<T: Term> {
    // interface for communicate with tty file
    ctl: EvLoopCtl,
    // store all tty output bytes
    history: Vec<u8>,
    // used by regex search history start
    last_buffer_start: usize,
    // Term decide how to decode output bytes
    phantom: PhantomData<T>,
}

enum ConsumeAction<T> {
    BreakValue(T),
    Continue,
}

impl<Tm> Tty<Tm>
where
    Tm: Term,
{
    pub fn new(ctl: EvLoopCtl) -> Self {
        Self {
            ctl,
            history: Vec::new(),
            last_buffer_start: 0,
            phantom: PhantomData {},
        }
    }

    pub fn stop(&self) {
        self.ctl.stop();
    }

    pub fn write(&mut self, s: &[u8], timeout: Duration) -> Result<()> {
        self.ctl
            .send_timeout(Req::Write(s.to_vec()), timeout)
            .map_err(|_| ConsoleError::Timeout)?;
        Ok(())
    }

    pub fn write_string(&mut self, s: &str, timeout: Duration) -> Result<()> {
        info!(msg = "write_string", s = s);
        self.write(s.as_bytes(), timeout)?;
        Ok(())
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String> {
        info!(msg = "wait_string_ntimes", pattern = pattern);
        self.comsume_buffer_and_map(timeout, |buffer, new| {
            {
                let buffer_str = Tm::parse_and_strip(buffer);
                let new_str = Tm::parse_and_strip(new);
                let res = count_substring(&buffer_str, pattern, repeat);
                info!(
                    msg = "wait_string_ntimes",
                    pattern = pattern,
                    repeat = repeat,
                    res = res,
                    new_buffer = new_str,
                );
                res.then_some(buffer_str)
            }
            .map_or(ConsumeAction::Continue, ConsumeAction::BreakValue)
        })
    }

    pub fn exec(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        info!(msg = "exec_global", cmd = cmd);
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        std::thread::sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!(6);
        let cmd = format!("{cmd}; echo $?{nanoid}{}", Tm::enter_input(),);
        let deadline = Instant::now() + timeout;
        self.write_string(&cmd, timeout)?;

        let match_left = &format!("{nanoid}{}{}", Tm::linebreak(), Tm::enter_input());
        let match_right = &format!("{nanoid}{}", Tm::linebreak());

        self.comsume_buffer_and_map(deadline - Instant::now(), |buffer, new| {
            // find target pattern from buffer
            let buffer_str = Tm::parse_and_strip(buffer);
            let new_str = Tm::parse_and_strip(new);
            info!(
                msg = "recv string",
                nanoid = nanoid,
                buffer_len = buffer.len(),
                new_buffer = new_str,
            );

            let Ok(catched_output) =
                t_util::assert_capture_between(&buffer_str, match_left, match_right)
            else {
                return ConsumeAction::BreakValue((1, "invalid consume regex".to_string()));
            };
            match catched_output {
                Some(v) => {
                    info!(msg = "catched_output", nanoid = nanoid, catched_output = v,);
                    if let Some((res, flag)) = v.rsplit_once(Tm::linebreak()) {
                        info!(
                            msg = "catched_output info",
                            nanoid = nanoid,
                            flag = flag,
                            res = res
                        );
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
                None => {
                    debug!(msg = "consume buffer continue");
                    ConsumeAction::Continue
                }
            }
        })
    }

    fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8], &[u8]) -> ConsumeAction<T>,
    ) -> Result<T> {
        let deadline = Instant::now() + timeout;

        let mut buffer_len = 0;
        loop {
            tracing::info!(msg = "deadline", deadline = ?(deadline - Instant::now()));
            // handle timeout
            if Instant::now() > deadline {
                break;
            }

            thread::sleep(Duration::from_millis(1000));

            // read buffer
            let res = self
                .ctl
                .send_timeout(Req::Read, Duration::from_millis(1000));
            match res {
                Ok(Res::Value(ref recv)) => {
                    if recv.is_empty() {
                        continue;
                    }

                    // save to history
                    self.history.extend(recv);
                    buffer_len += recv.len();

                    debug!(
                        msg = "event loop recv",
                        sum_buffer_len = self.history.len() - self.last_buffer_start,
                        last_buffer_start = self.last_buffer_start,
                        old_buffer_len = self.history.len() - buffer_len,
                        new_buffer_len = buffer_len,
                        new_buffer_acc = recv.len(),
                    );

                    // find target pattern
                    let res = f(&self.history[self.last_buffer_start..], recv);

                    match res {
                        ConsumeAction::BreakValue(v) => {
                            // cut from last find
                            info!(msg = "buffer cut");
                            self.last_buffer_start = self.history.len() - buffer_len;
                            return Ok(v);
                        }
                        ConsumeAction::Continue => {
                            continue;
                        }
                    }
                }
                Ok(res) => {
                    error!(msg = "invalid msg varient", res = ?res);
                    break;
                }
                Err(e) => match e {
                    std::sync::mpsc::RecvTimeoutError::Timeout => {}
                    std::sync::mpsc::RecvTimeoutError::Disconnected => {
                        error!(msg = "recv failed");
                        break;
                    }
                },
            }
        }
        Err(ConsoleError::Timeout)
    }
}

fn count_substring(s: &str, substring: &str, n: usize) -> bool {
    let mut count = 0;
    let mut start = 0;

    while let Some(pos) = s[start..].find(substring) {
        count += 1;
        if count == n {
            return true;
        }
        start += pos + substring.len();
    }

    false
}

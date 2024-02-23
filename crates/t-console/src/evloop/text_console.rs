use crate::{term::Term, EvLoopCtl, Req, Res};
use std::{
    fmt::Display,
    marker::PhantomData,
    sync::mpsc,
    time::{Duration, Instant},
};
use tracing::{debug, error, info};

#[derive(Debug)]
pub enum ConsoleError {
    Io(std::io::Error),
    ConnectionBroken,
    Timeout,
}

impl Display for ConsoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ConsoleError::Io(e) => write!(f, "IO: {}", e),
            ConsoleError::ConnectionBroken => write!(f, "Connection broken"),
            ConsoleError::Timeout => write!(f, "Timeout"),
        }
    }
}

type Result<T> = std::result::Result<T, ConsoleError>;

pub struct BufEvLoopCtl<T: Term> {
    ctl: EvLoopCtl,
    history: Vec<u8>,
    last_buffer_start: usize,
    phantom: PhantomData<T>,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
    tx: mpsc::Sender<Vec<u8>>,
}

pub struct MsgStream {
    inner: mpsc::Receiver<Vec<u8>>,
}

impl Iterator for MsgStream {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.recv().ok()
    }
}

impl<Tm> BufEvLoopCtl<Tm>
where
    Tm: Term,
{
    pub fn new(ctl: EvLoopCtl) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            ctl,
            history: Vec::new(),
            last_buffer_start: 0,
            phantom: PhantomData {},
            tx,
            rx: Some(rx),
        }
    }

    pub fn take_stream(&mut self) -> MsgStream {
        let Some(rx) = self.rx.take() else {
            panic!("already taken");
        };
        MsgStream { inner: rx }
    }

    pub fn history(&self) -> Vec<u8> {
        match self.ctl.send(Req::Dump) {
            Ok(Res::Value(v)) => v,
            Ok(_v) => self.history.clone(),
            Err(_e) => self.history.clone(),
        }
    }

    pub fn send(&self, req: Req) -> std::result::Result<Res, mpsc::RecvError> {
        self.ctl.send(req)
    }

    pub fn write(&mut self, s: &[u8]) -> Result<()> {
        self.ctl
            .send(Req::Write(s.to_vec()))
            .map_err(|_| ConsoleError::ConnectionBroken)?;
        Ok(())
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        debug!(msg = "write_string", s = s);
        self.ctl
            .send(Req::Write(s.as_bytes().to_vec()))
            .map_err(|_| ConsoleError::ConnectionBroken)?;
        Ok(())
    }

    pub fn wait_string_ntimes(
        &mut self,
        timeout: Duration,
        pattern: &str,
        repeat: usize,
    ) -> Result<String> {
        self.comsume_buffer_and_map(timeout, |buffer| {
            let buffer_str = Tm::parse_and_strip(buffer);
            let res = count_substring(&buffer_str, pattern, repeat);
            info!(
                msg = "wait_string_ntimes",
                pattern = pattern,
                repeat = repeat,
                res = res,
                buffer = buffer_str,
            );
            res.then_some(buffer_str)
        })
    }

    pub fn exec_global(&mut self, timeout: Duration, cmd: &str) -> Result<(i32, String)> {
        // wait for prompt show, cmd may write too fast before prompt show, which will broken regex
        std::thread::sleep(Duration::from_millis(70));

        let nanoid = nanoid::nanoid!(6);
        let cmd = format!("{cmd}; echo $?{nanoid}{}", Tm::enter_input(),);
        self.write_string(&cmd)?;

        let match_left = &format!("{nanoid}{}{}", Tm::linebreak(), Tm::enter_input());
        let match_right = &format!("{nanoid}{}", Tm::linebreak());

        self.comsume_buffer_and_map_inner(timeout, |buffer, _new| {
            // find target pattern from buffer
            let parsed_str = Tm::parse_and_strip(buffer);
            info!(
                msg = "recv string",
                nanoid = nanoid,
                buffer_len = buffer.len(),
                parsed_str_len = parsed_str.len(),
                parsed_str = parsed_str,
            );

            let Ok(catched_output) =
                t_util::assert_capture_between(&parsed_str, match_left, match_right)
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

    pub fn comsume_buffer_and_map<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<T> {
        self.comsume_buffer_and_map_inner(timeout, |bytes, _new| {
            f(bytes).map_or(ConsumeAction::Continue, |v| ConsumeAction::BreakValue(v))
        })
    }

    fn comsume_buffer_and_map_inner<T>(
        &mut self,
        timeout: Duration,
        f: impl Fn(&[u8], &[u8]) -> ConsumeAction<T>,
    ) -> Result<T> {
        let deadline = Instant::now() + timeout;

        let mut buffer_len = 0;
        loop {
            // debug!(msg = "deadline", deadline = ?(deadline - Instant::now()));
            // handle timeout
            if Instant::now() > deadline {
                break;
            }

            // read buffer
            let res = self.ctl.send_timeout(Req::Read, timeout);
            match res {
                Ok(Res::Value(ref recv)) => {
                    if recv.is_empty() {
                        continue;
                    }

                    // send to read time receiver
                    let _ = self.tx.send(recv.to_vec());

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
                Ok(t) => {
                    error!(msg = "invalid msg varient", t = ?t);
                    panic!();
                }
                Err(_e) => {
                    error!(msg = "recv timeout");
                    break;
                }
            }
        }
        Err(ConsoleError::Timeout)
    }
}

enum ConsumeAction<T> {
    BreakValue(T),
    Continue,
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

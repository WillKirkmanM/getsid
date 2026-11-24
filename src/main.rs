use clap::Parser;
use std::process::ExitCode;

#[derive(Parser)]
#[command(version, about = "Display Linux process session IDs")]
struct Arguments {
    #[arg(
        long,
        help = "Exit 0 if all queried processes share a session, or 1 if they differ"
    )]
    same_session: bool,

    #[arg(
        value_name = "PID",
        value_parser = parse_pid,
        allow_negative_numbers = true,
        help = "Process IDs to query; 0 selects the calling process"
    )]
    pids: Vec<i32>,
}

fn parse_pid(value: &str) -> Result<i32, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "invalid process ID '{value}': expected an integer from 0 to {}",
            i32::MAX
        ));
    }

    value.parse::<i32>().map_err(|_| {
        format!(
            "process ID '{value}' is out of range: expected 0 to {}",
            i32::MAX
        )
    })
}

#[cfg(target_os = "linux")]
fn session_id(pid: i32) -> std::io::Result<i32> {
    unsafe extern "C" {
        fn getsid(pid: std::ffi::c_int) -> std::ffi::c_int;
    }

    let session = unsafe { getsid(pid) };
    if session == -1 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(session)
    }
}

#[cfg(target_os = "linux")]
fn run(arguments: Arguments) -> ExitCode {
    use std::io::{self, Write};

    let implicit_pid = arguments.pids.is_empty();
    let pids = if implicit_pid {
        vec![0]
    } else {
        arguments.pids
    };
    let mut first_session = None;
    let mut different_sessions = false;
    let mut failed = false;
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut output_failed = false;

    for pid in pids {
        match session_id(pid) {
            Ok(session) => {
                if let Some(first) = first_session {
                    different_sessions |= first != session;
                } else {
                    first_session = Some(session);
                }

                if !output_failed {
                    let result = if implicit_pid {
                        writeln!(output, "{session}")
                    } else {
                        writeln!(output, "{pid} {session}")
                    };
                    if let Err(error) = result {
                        eprintln!("getsid: cannot write output: {error}");
                        output_failed = true;
                        failed = true;
                    }
                }
            }
            Err(error) => {
                eprintln!("getsid: cannot query process {pid}: {error}");
                failed = true;
            }
        }
    }

    if !output_failed && let Err(error) = output.flush() {
        eprintln!("getsid: cannot flush output: {error}");
        failed = true;
    }

    if failed || (arguments.same_session && different_sessions) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(not(target_os = "linux"))]
fn run(_arguments: Arguments) -> ExitCode {
    eprintln!("getsid: unsupported platform: this command requires Linux");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    run(Arguments::parse())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_zero_and_pid_boundaries() {
        assert_eq!(parse_pid("0"), Ok(0));
        assert_eq!(parse_pid("1"), Ok(1));
        assert_eq!(parse_pid("00042"), Ok(42));
        assert_eq!(parse_pid("2147483647"), Ok(i32::MAX));
    }

    #[test]
    fn rejects_malformed_and_out_of_range_pids() {
        for value in [
            "",
            "-1",
            "+1",
            "invalid",
            "1.5",
            " 1",
            "1 ",
            "2147483648",
            "999999999999999999999999999999",
        ] {
            assert!(parse_pid(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn parses_multiple_pids_with_session_comparison() {
        let arguments = Arguments::try_parse_from(["getsid", "--same-session", "0", "1"])
            .expect("valid arguments");
        assert!(arguments.same_session);
        assert_eq!(arguments.pids, vec![0, 1]);
    }

    #[test]
    fn rejects_invalid_pids_during_argument_parsing() {
        for value in ["-1", "invalid", "2147483648"] {
            assert!(Arguments::try_parse_from(["getsid", value]).is_err());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn zero_and_current_pid_identify_the_same_session() {
        let pid = i32::try_from(std::process::id()).expect("Linux process ID fits i32");
        let current = session_id(0).expect("calling process has a session");
        assert!(current > 0);
        assert_eq!(
            session_id(pid).expect("current process is queryable"),
            current
        );
    }
}

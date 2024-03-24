class Driver:
    """
    A driver for running test

    :param toml_str: toml config string
    """

    def __init__(self, toml_str: str) -> Driver: ...
    def start(self):
        """
        start the runner
        """

    def stop(self):
        """
        stop the runner
        """

    def sleep(self, secs: int):
        """
        sleep for secs, you can use this function to simulate a long running script
        """

    def get_env(self, key: str) -> str | None:
        """
        get environment variable by key from toml env section
        """

    def assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in console, return stdout, throw exception if return code is not 0
        """

    def script_run(self, cmd: str, timeout: int) -> str:
        """
        like assert_script_run, but not throw exception if return code is not 0
        """

    def write(self, s: str):
        """
        write string to console
        """

    def writeln(self, s: str):
        """
        write string with '\n' to console
        """

    def wait_string_ntimes(self, s: str, n: int, timeout: int) -> bool:
        """
        wait pattern in console output show n times
        """

    def assert_wait_string_ntimes(self, s: str, n: int, timeout: int):
        """
        wait pattern in console output, if timeout, throw error
        """

    def ssh_assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in ssh, return stdout, throw exception if return code is not 0
        """

    def ssh_script_run(self, cmd: str, timeout: int) -> str:
        """
        like ssh_assert_script_run, but not throw exception if return code is not 0
        """

    def ssh_write(self, s: str):
        """
        write string to ssh console
        """

    def ssh_assert_script_run_seperate(self, cmd: str, timeout: int) -> str:
        """
        run script in seperate ssh session, return stdout, throw exception if return code is not 0
        """

    def serial_assert_script_run(self, cmd: str, timeout: int) -> str:
        """
        run script in global ssh session, return stdout, throw exception if return code is not 0
        """

    def serial_script_run(self, cmd: str, timeout: int) -> str:
        """
        like serial_assert_script_run, but not throw exception if return code is not 0
        """

    def serial_write(self, s: str):
        """
        write string to ssh console
        """

    def assert_screen(self, tag: str, timeout: int):
        """
        check screen, throw exception if timeout, or not similar to tag
        """

    def check_screen(self, tag: str, timeout: int) -> bool:
        """
        check screen, return false if timeout, or not similar to tag
        """

    def vnc_type_string(self, s: str):
        """
        type string
        """

    def vnc_send_key(self):
        """
        send event
        """

    def vnc_refresh(self):
        """
        force refresh
        """

    def mouse_click(self):
        """
        click mouse
        """

    def mouse_rclick(self):
        """
        click mouse right button
        """

    def mouse_keydown(self):
        """
        mouse left button down
        """

    def mouse_keyup(self):
        """
        mouse left button up
        """

    def mouse_move(self, x: int, y: int):
        """
        move mouse to x, y
        """

    def mouse_hide(self):
        """
        hide mouse
        """

#!/usr/bin/env python3
"""Send an event only to a test CLI's dedicated console, never the CI console."""
import ctypes
from ctypes import wintypes
import sys
import os
import subprocess
import threading
import time


def send(pid: int, event: str) -> None:
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.FreeConsole()
    kernel.AttachConsole.argtypes = [wintypes.DWORD]
    kernel.AttachConsole.restype = wintypes.BOOL
    if not kernel.AttachConsole(pid):
        raise ctypes.WinError(ctypes.get_last_error())
    handler_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.DWORD)
    ignore = handler_type(lambda _event: True)
    kernel.SetConsoleCtrlHandler.argtypes = [handler_type, wintypes.BOOL]
    if not kernel.SetConsoleCtrlHandler(ignore, True):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        if event == "close":
            kernel.GetConsoleWindow.restype = wintypes.HWND
            window = kernel.GetConsoleWindow()
            if not window:
                raise RuntimeError("Test console has no window")
            user = ctypes.WinDLL("user32", use_last_error=True)
            user.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
            if not user.PostMessageW(window, 0x0010, 0, 0):  # WM_CLOSE
                raise ctypes.WinError(ctypes.get_last_error())
        else:
            kernel.GenerateConsoleCtrlEvent.argtypes = [wintypes.DWORD, wintypes.DWORD]
            if not kernel.GenerateConsoleCtrlEvent(0 if event == "ctrl-c" else 1, 0):
                raise ctypes.WinError(ctypes.get_last_error())
            time.sleep(0.1)
    finally:
        kernel.FreeConsole()


if __name__ == "__main__":
    send(int(sys.argv[1]), sys.argv[2])


class PseudoConsoleProcess:
    """Own a real ConPTY so close tests work without an interactive desktop.

    GetConsoleWindow can be only a message-queue placeholder on a hosted runner;
    posting WM_CLOSE to it is not evidence of terminal closure. Closing an owned
    pseudoconsole uses the same kernel API as an actual terminal host.
    """
    def __init__(self, argv, cwd, env):
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel = kernel
        self.returncode = None
        self.output = bytearray()
        self.console = wintypes.HANDLE()

        class Coord(ctypes.Structure):
            _fields_ = [("X", ctypes.c_short), ("Y", ctypes.c_short)]

        class Startup(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("reserved", wintypes.LPWSTR),
                        ("desktop", wintypes.LPWSTR), ("title", wintypes.LPWSTR),
                        ("x", wintypes.DWORD), ("y", wintypes.DWORD),
                        ("width", wintypes.DWORD), ("height", wintypes.DWORD),
                        ("xChars", wintypes.DWORD), ("yChars", wintypes.DWORD),
                        ("fill", wintypes.DWORD), ("flags", wintypes.DWORD),
                        ("show", wintypes.WORD), ("reservedSize", wintypes.WORD),
                        ("reservedBytes", ctypes.c_void_p), ("stdin", wintypes.HANDLE),
                        ("stdout", wintypes.HANDLE), ("stderr", wintypes.HANDLE)]

        class StartupEx(ctypes.Structure):
            _fields_ = [("startup", Startup), ("attributes", ctypes.c_void_p)]

        class ProcessInfo(ctypes.Structure):
            _fields_ = [("process", wintypes.HANDLE), ("thread", wintypes.HANDLE),
                        ("pid", wintypes.DWORD), ("tid", wintypes.DWORD)]

        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CreatePipe.argtypes = [ctypes.POINTER(wintypes.HANDLE), ctypes.POINTER(wintypes.HANDLE), ctypes.c_void_p, wintypes.DWORD]
        kernel.CreatePseudoConsole.argtypes = [Coord, wintypes.HANDLE, wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
        kernel.CreatePseudoConsole.restype = ctypes.c_long
        kernel.ClosePseudoConsole.argtypes = [wintypes.HANDLE]
        kernel.InitializeProcThreadAttributeList.argtypes = [ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, ctypes.POINTER(ctypes.c_size_t)]
        kernel.UpdateProcThreadAttribute.argtypes = [ctypes.c_void_p, wintypes.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]
        kernel.DeleteProcThreadAttributeList.argtypes = [ctypes.c_void_p]
        kernel.CreateProcessW.argtypes = [wintypes.LPCWSTR, wintypes.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, wintypes.BOOL,
                                        wintypes.DWORD, ctypes.c_void_p, wintypes.LPCWSTR, ctypes.POINTER(StartupEx), ctypes.POINTER(ProcessInfo)]
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        kernel.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
        kernel.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]

        input_read, self.input_write = wintypes.HANDLE(), wintypes.HANDLE()
        self.output_read, output_write = wintypes.HANDLE(), wintypes.HANDLE()
        if not kernel.CreatePipe(ctypes.byref(input_read), ctypes.byref(self.input_write), None, 0):
            raise ctypes.WinError(ctypes.get_last_error())
        if not kernel.CreatePipe(ctypes.byref(self.output_read), ctypes.byref(output_write), None, 0):
            raise ctypes.WinError(ctypes.get_last_error())
        status = kernel.CreatePseudoConsole(Coord(80, 25), input_read, output_write, 0, ctypes.byref(self.console))
        kernel.CloseHandle(input_read)
        kernel.CloseHandle(output_write)
        if status < 0:
            raise OSError(f"CreatePseudoConsole failed: {status:#x}")
        self.reader = threading.Thread(target=self._drain, daemon=True)
        self.reader.start()
        size = ctypes.c_size_t()
        kernel.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
        attributes = ctypes.create_string_buffer(size.value)
        if not kernel.InitializeProcThreadAttributeList(attributes, 1, 0, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            # PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE takes HPCON itself as lpValue.
            if not kernel.UpdateProcThreadAttribute(attributes, 0, 0x00020016, self.console,
                                                     ctypes.sizeof(wintypes.HANDLE), None, None):
                raise ctypes.WinError(ctypes.get_last_error())
            startup = StartupEx()
            startup.startup.cb = ctypes.sizeof(StartupEx)
            startup.attributes = ctypes.cast(attributes, ctypes.c_void_p)
            process = ProcessInfo()
            environment = ctypes.create_unicode_buffer("\0".join(f"{key}={value}" for key, value in sorted(env.items())) + "\0\0")
            command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
            if not kernel.CreateProcessW(argv[0], command, None, None, False, 0x00080000 | 0x00000400,
                                         environment, str(cwd), ctypes.byref(startup), ctypes.byref(process)):
                raise ctypes.WinError(ctypes.get_last_error())
            self.process = process.process
            self.pid = process.pid
            kernel.CloseHandle(process.thread)
        finally:
            kernel.DeleteProcThreadAttributeList(attributes)

    def _drain(self):
        buffer = ctypes.create_string_buffer(4096)
        count = wintypes.DWORD()
        while self.kernel.ReadFile(self.output_read, buffer, len(buffer), ctypes.byref(count), None):
            self.output.extend(buffer.raw[:count.value])
            if len(self.output) > 65536:
                del self.output[:-65536]

    def close_console(self):
        if self.console.value:
            self.kernel.ClosePseudoConsole(self.console)
            self.console = wintypes.HANDLE()

    def poll(self):
        if self.returncode is None and self.kernel.WaitForSingleObject(self.process, 0) == 0:
            code = wintypes.DWORD()
            if not self.kernel.GetExitCodeProcess(self.process, ctypes.byref(code)):
                raise ctypes.WinError(ctypes.get_last_error())
            self.returncode = code.value
        return self.returncode

    def communicate(self, timeout):
        self.wait(timeout)
        return bytes(self.output), b""

    def wait(self, timeout):
        deadline = time.monotonic() + timeout
        while self.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError("ConPTY child did not exit")
            time.sleep(0.01)
        return self.returncode

    def kill(self):
        if self.poll() is None:
            self.kernel.TerminateProcess(self.process, 1)

    def release(self):
        self.close_console()
        self.kernel.CloseHandle(self.input_write)
        self.reader.join(timeout=2)
        self.kernel.CloseHandle(self.output_read)
        self.kernel.CloseHandle(self.process)

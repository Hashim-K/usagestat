#!/usr/bin/env python3
"""Send an event only to a test CLI's dedicated console, never the CI console."""
import ctypes
from ctypes import wintypes
import sys
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

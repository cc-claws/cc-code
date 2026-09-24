"""Run the ignored hover stress test in an isolated real Windows ConPTY.

Usage: python scripts/test-hover-conpty.py <test-binary.exe> <new-evidence.json>
Does not launch the app, use credentials, or touch the user's active console.
PERI_EXPECT_HOVER=0/1 asserts the detected host capability. PERI_CONPTY_DLL
optionally selects Microsoft's ConPTY SDK (OpenConsole.exe must be adjacent).
"""
import ctypes as c
from ctypes import wintypes as w
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import threading
import time
import struct


class COORD(c.Structure):
    _fields_ = [("X", c.c_short), ("Y", c.c_short)]


class STARTUPINFO(c.Structure):
    _fields_ = [("cb", w.DWORD), ("lpReserved", w.LPWSTR), ("lpDesktop", w.LPWSTR),
                ("lpTitle", w.LPWSTR), ("dwX", w.DWORD), ("dwY", w.DWORD),
                ("dwXSize", w.DWORD), ("dwYSize", w.DWORD), ("dwXCountChars", w.DWORD),
                ("dwYCountChars", w.DWORD), ("dwFillAttribute", w.DWORD),
                ("dwFlags", w.DWORD), ("wShowWindow", w.WORD), ("cbReserved2", w.WORD),
                ("lpReserved2", c.c_void_p), ("hStdInput", w.HANDLE),
                ("hStdOutput", w.HANDLE), ("hStdError", w.HANDLE)]


class STARTUPINFOEX(c.Structure):
    _fields_ = [("StartupInfo", STARTUPINFO), ("lpAttributeList", c.c_void_p)]


class PROCESS_INFORMATION(c.Structure):
    _fields_ = [("hProcess", w.HANDLE), ("hThread", w.HANDLE),
                ("dwProcessId", w.DWORD), ("dwThreadId", w.DWORD)]


def native_probe():
    k = c.WinDLL("kernel32", use_last_error=True)
    k.CreateFileW.argtypes = [w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE]
    k.CreateFileW.restype = w.HANDLE
    k.SetConsoleMode.argtypes = [w.HANDLE, w.DWORD]
    k.WriteConsoleW.argtypes = [w.HANDLE, w.LPCWSTR, w.DWORD, c.POINTER(w.DWORD), c.c_void_p]
    k.ReadConsoleInputW.argtypes = [w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]
    inp = k.CreateFileW("CONIN$", 0xC0000000, 3, None, 3, 0, None)
    out = k.CreateFileW("CONOUT$", 0xC0000000, 3, None, 3, 0, None)
    k.SetConsoleMode(inp, 0x88)
    k.SetConsoleMode(inp, 0x98)
    k.SetConsoleMode(out, 7)
    n = w.DWORD()
    hover = os.environ.get("PERI_EXPECT_HOVER") != "0"
    modes = "\x1b[?1003h" if hover else "\x1b[?1003l\x1b[?1000h\x1b[?1002h"
    if os.environ.get("PERI_HOVER_PROBE_BASELINE"):
        modes = "\x1b[?1000h\x1b[?1002h\x1b[?1015h"
    msg = f"\x1b[?1049h{modes}\x1b[?1006hHOVER_READY:{int(hover)}\r\n"
    k.WriteConsoleW(out, msg, len(msg), c.byref(n), None)
    records = []
    done = False
    while not done:
        buf = c.create_string_buffer(20 * 256)
        if not k.ReadConsoleInputW(inp, buf, 256, c.byref(n)):
            raise c.WinError(c.get_last_error())
        for i in range(n.value):
            rec = buf.raw[i*20:(i+1)*20]
            kind = struct.unpack_from("<H", rec)[0]
            if kind == 1:
                data = struct.unpack_from("<IHHHHI", rec, 4)
                done |= data[0] != 0 and data[4] == 13
            else:
                data = list(rec[4:])
            records.append((kind, data))
    Path(os.environ["PERI_HOVER_TEST_RESULT"]).write_text(json.dumps(records), encoding="utf-8")


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    evidence = Path(sys.argv[2]).resolve()
    if evidence.exists():
        raise RuntimeError(f"Refusing to overwrite evidence: {evidence}")
    k = c.WinDLL("kernel32", use_last_error=True)
    if os.environ.get("PERI_CONPTY_DLL"):
        conpty = c.WinDLL(os.environ["PERI_CONPTY_DLL"], use_last_error=True)
        k.CreatePseudoConsole = conpty.ConptyCreatePseudoConsole
        k.ClosePseudoConsole = conpty.ConptyClosePseudoConsole
    k.CreatePipe.argtypes = [c.POINTER(w.HANDLE), c.POINTER(w.HANDLE), c.c_void_p, w.DWORD]
    k.CreatePseudoConsole.argtypes = [COORD, w.HANDLE, w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]
    k.CreatePseudoConsole.restype = c.c_long
    k.InitializeProcThreadAttributeList.argtypes = [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(c.c_size_t)]
    k.UpdateProcThreadAttribute.argtypes = [c.c_void_p, w.DWORD, c.c_size_t, c.c_void_p, c.c_size_t, c.c_void_p, c.c_void_p]
    k.CreateProcessW.argtypes = [w.LPCWSTR, w.LPWSTR, c.c_void_p, c.c_void_p, w.BOOL, w.DWORD, c.c_void_p, w.LPCWSTR, c.POINTER(STARTUPINFOEX), c.POINTER(PROCESS_INFORMATION)]
    k.ReadFile.argtypes = [w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.c_void_p]
    k.WriteFile.argtypes = k.ReadFile.argtypes
    k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]
    k.GetExitCodeProcess.argtypes = [w.HANDLE, c.POINTER(w.DWORD)]
    k.TerminateProcess.argtypes = [w.HANDLE, w.UINT]
    k.CloseHandle.argtypes = [w.HANDLE]
    k.ClosePseudoConsole.argtypes = [w.HANDLE]
    k.DeleteProcThreadAttributeList.argtypes = [c.c_void_p]

    def check(ok):
        if not ok:
            raise c.WinError(c.get_last_error())

    ir, iw, out_r, out_w, pc = (w.HANDLE() for _ in range(5))
    pi = PROCESS_INFORMATION()
    attrs = None
    transcript = bytearray()
    ready = threading.Event()
    try:
        check(k.CreatePipe(c.byref(ir), c.byref(iw), None, 0))
        check(k.CreatePipe(c.byref(out_r), c.byref(out_w), None, 0))
        hr = k.CreatePseudoConsole(COORD(100, 30), ir, out_w, 0, c.byref(pc))
        if hr != 0:
            raise RuntimeError(f"CreatePseudoConsole failed: {hr:#x}")
        k.CloseHandle(ir)
        ir = w.HANDLE()
        k.CloseHandle(out_w)
        out_w = w.HANDLE()
        size = c.c_size_t()
        k.InitializeProcThreadAttributeList(None, 1, 0, c.byref(size))
        attrs = c.create_string_buffer(size.value)
        check(k.InitializeProcThreadAttributeList(attrs, 1, 0, c.byref(size)))
        check(k.UpdateProcThreadAttribute(attrs, 0, 0x00020016, pc, c.sizeof(pc), None, None))
        si = STARTUPINFOEX()
        si.StartupInfo.cb = c.sizeof(si)
        si.lpAttributeList = c.cast(attrs, c.c_void_p)
        os.environ["PERI_HOVER_CONPTY_TEST"] = "1"
        os.environ["PERI_HOVER_TEST_RESULT"] = str(evidence)
        os.environ["RUST_LOG_FILE"] = str(evidence) + ".log"
        os.environ["RUST_LOG"] = "debug"
        args = [binary, "--ignored", "--exact", "event::hover_console_test::test_hover_conpty_flood_does_not_become_keyboard_input", "--nocapture", "--test-threads=1"]
        if os.environ.get("PERI_HOVER_NATIVE_PROBE"):
            args = [sys.executable, str(Path(__file__).resolve()), "--native-probe"]
        command = c.create_unicode_buffer(subprocess.list2cmdline(args))
        check(k.CreateProcessW(None, command, None, None, False, 0x00080000, None, None, c.byref(si), c.byref(pi)))

        def drain():
            buf = c.create_string_buffer(65536)
            n = w.DWORD()
            while k.ReadFile(out_r, buf, len(buf), c.byref(n), None) and n.value:
                transcript.extend(buf.raw[:n.value])
                if b"HOVER_READY:0" in transcript or b"HOVER_READY:1" in transcript:
                    ready.set()

        threading.Thread(target=drain, daemon=True).start()
        if not ready.wait(30):
            raise RuntimeError(f"Test did not become ready: {transcript[-5000:].decode(errors='replace')}")

        def send(data):
            n = w.DWORD()
            check(k.WriteFile(iw, data, len(data), c.byref(n), None))
            if n.value != len(data):
                raise RuntimeError("Partial input write")

        hover = b"HOVER_READY:1" in transcript
        frontend_hover = False
        for modes, operation in re.findall(rb"\x1b\[\?([0-9;]+)([hl])", bytes(transcript)):
            if b"1003" in modes.split(b";"):
                frontend_hover = operation == b"h"
        if frontend_hover != hover and not os.environ.get("PERI_HOVER_NATIVE_PROBE"):
            raise RuntimeError(f"Host capability and emitted terminal mode disagree: capability={hover}, frontend={frontend_hover}; output={bytes(transcript[:2000])!r}")
        expected = os.environ.get("PERI_EXPECT_HOVER")
        if expected is not None and hover != (expected == "1"):
            raise RuntimeError(f"Unexpected hover capability: {hover}; expected={expected}; output={bytes(transcript[:4000])!r}")
        send(b"before")
        def mouse_report(x, y):
            if os.environ.get("PERI_MOUSE_PROTOCOL") == "x10":
                return b"\x1b[M" + bytes([67, x + 32, y + 32])
            return f"\x1b[<35;{x};{y}M".encode()
        # A frontend reports hover only when the application requests it.
        injected = 0
        if hover:
            batches = int(os.environ.get("PERI_HOVER_BATCHES", "200"))
            for batch in range(batches):
                send(b"".join(mouse_report(1 + i % 80, 1 + i % 20) for i in range(batch * 100, (batch + 1) * 100)))
                time.sleep(float(os.environ.get("PERI_HOVER_BATCH_DELAY", "0")))
            send(mouse_report(80, 10))
            injected = batches * 100 + 1
        send(b"\x1b[<0;80;10M\x1b[<32;80;11M\x1b[<0;80;11m\x1b[<64;80;11M")
        send(b"after\r")
        if k.WaitForSingleObject(pi.hProcess, 30000) != 0:
            raise RuntimeError("Stress test timed out")
        exit_code = w.DWORD()
        check(k.GetExitCodeProcess(pi.hProcess, c.byref(exit_code)))
        if exit_code.value != 0:
            raise RuntimeError(f"Test failed: exit={exit_code.value}; output-start={bytes(transcript[:2000])!r}; output-end={transcript[-1000:].decode(errors='replace')}")
        result = json.loads(evidence.read_text(encoding="utf-8"))
        print(json.dumps({"native_records": len(result)} if isinstance(result, list) else {"injected_moves": injected, **result}, ensure_ascii=False, indent=2))
    finally:
        if pi.hProcess:
            if k.WaitForSingleObject(pi.hProcess, 0) != 0:
                k.TerminateProcess(pi.hProcess, 1)
                k.WaitForSingleObject(pi.hProcess, 5000)
        if pc:
            k.ClosePseudoConsole(pc)
        if attrs:
            k.DeleteProcThreadAttributeList(attrs)
        for handle in [ir, iw, out_r, out_w, pi.hThread, pi.hProcess]:
            if handle:
                k.CloseHandle(handle)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--native-probe":
        native_probe()
    else:
        main()

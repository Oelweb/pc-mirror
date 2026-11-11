"""Elternfenster für Tests des eingebetteten Videos (wie das Fenster der App)."""
import queue
import threading


def host_window():
    """Ein Fenster als Elternfenster für das Video (wie das Fenster der App),
    mit eigener Nachrichtenschleife in einem Thread."""
    import ctypes
    from ctypes import wintypes
    user32 = ctypes.windll.user32
    user32.CreateWindowExW.restype = wintypes.HWND
    user32.CreateWindowExW.argtypes = [wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD,
                                       ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                       wintypes.HWND, wintypes.HMENU, wintypes.HINSTANCE, wintypes.LPVOID]
    created = queue.Queue()

    def run():
        hwnd = user32.CreateWindowExW(0, "STATIC", "PC-Mirror E2E", 0x10000000 | 0x00CF0000,
                                      50, 50, 640, 480, None, None, None, None)
        created.put(hwnd)
        msg = wintypes.MSG()
        while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            user32.TranslateMessage(ctypes.byref(msg))
            user32.DispatchMessageW(ctypes.byref(msg))

    threading.Thread(target=run, daemon=True).start()
    return created.get(timeout=5)

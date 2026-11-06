"""Test the actual hidden Windows child viewport with synthetic decoded frames.

This is a native geometry/IPC test, not a physical AirPlay device test.
"""
import argparse
import ctypes as c
from ctypes import wintypes as w
import math
import os
from pathlib import Path
import subprocess
import threading
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--package', type=Path, required=True)
package = parser.parse_args().package.resolve()
u = c.WinDLL('user32', use_last_error=True)
u.CreateWindowExW.argtypes = [w.DWORD,w.LPCWSTR,w.LPCWSTR,w.DWORD,c.c_int,c.c_int,c.c_int,c.c_int,w.HWND,w.HMENU,w.HINSTANCE,c.c_void_p]
u.CreateWindowExW.restype = w.HWND
u.FindWindowExW.argtypes = [w.HWND,w.HWND,w.LPCWSTR,w.LPCWSTR]
u.FindWindowExW.restype = w.HWND
u.GetClientRect.argtypes = [w.HWND,c.POINTER(w.RECT)]
u.GetWindowRect.argtypes = [w.HWND,c.POINTER(w.RECT)]
u.MapWindowPoints.argtypes = [w.HWND,w.HWND,c.POINTER(w.POINT),w.UINT]
u.SetWindowPos.argtypes = [w.HWND,w.HWND,c.c_int,c.c_int,c.c_int,c.c_int,w.UINT]
u.DestroyWindow.argtypes = [w.HWND]
u.PeekMessageW.argtypes = [c.POINTER(w.MSG),w.HWND,w.UINT,w.UINT,w.UINT]
u.TranslateMessage.argtypes = [c.POINTER(w.MSG)]
u.DispatchMessageW.argtypes = [c.POINTER(w.MSG)]
u.DispatchMessageW.restype = c.c_ssize_t
u.SetProcessDpiAwarenessContext.argtypes = [c.c_void_p]
u.SetProcessDpiAwarenessContext(c.c_void_p(-4))
parent = u.CreateWindowExW(0,'STATIC','PC-Mirror hidden test',0x02CF0000,0,0,800,600,None,None,None,None)
assert parent

def pump():
    message = w.MSG()
    while u.PeekMessageW(c.byref(message),None,0,0,1):
        u.TranslateMessage(c.byref(message));u.DispatchMessageW(c.byref(message))

def expected(fill):
    r = w.RECT(); assert u.GetClientRect(parent,c.byref(r))
    width,height=r.right,r.bottom
    cw=max(width,math.ceil(height*360/640)) if fill else width
    ch=max(height,math.ceil(width*640/360)) if fill else height
    return -((cw-width)//2),-((ch-height)//2),cw,ch

def wait_view(fill):
    deadline=time.monotonic()+8
    while time.monotonic()<deadline:
        pump()
        child=u.FindWindowExW(parent,None,'PCMirrorVideoChild',None)
        if child:
            r=w.RECT();u.GetWindowRect(child,c.byref(r))
            points=(w.POINT*2)(w.POINT(r.left,r.top),w.POINT(r.right,r.bottom))
            u.MapWindowPoints(None,parent,points,2)
            actual=(points[0].x,points[0].y,points[1].x-points[0].x,points[1].y-points[0].y)
            want=expected(fill)
            if actual==want:return
        time.sleep(.05)
    raise AssertionError(f'Viewport {actual}, expected {want}')

try:
    for model in ('iPad14,3','iPhone16,1','Unknown'):
        env=dict(os.environ,PCMIRROR_EMBEDDED='1',PCMIRROR_TEST_HEADLESS='1',PCMIRROR_HWND=str(parent),PCMIRROR_TEST_MODEL=model,PCMIRROR_IPAD_FILL='0')
        system=os.environ.get('SYSTEMROOT','C:/Windows')
        env.update(PATH=str(package)+';'+system+'/System32;'+system,
            GST_PLUGIN_SYSTEM_PATH_1_0=str(package/'lib/gstreamer-1.0'),GST_PLUGIN_PATH_1_0='',
            GST_PLUGIN_SCANNER_1_0=str(package/'libexec/gstreamer-1.0/gst-plugin-scanner.exe'))
        process=subprocess.Popen([str(package/'uxplay.exe'),'--pcmirror-selftest'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,env=env,creationflags=subprocess.CREATE_NO_WINDOW)
        output=[]
        thread=threading.Thread(target=lambda:output.extend(process.stdout),daemon=True);thread.start()
        try:
            deadline=time.monotonic()+8
            while not any('PCMIRROR|FRAME|360|640' in line for line in output):
                pump()
                assert time.monotonic()<deadline,output
                time.sleep(.05)
            wait_view(False)
            for fill in (1,0,1):
                acknowledgement=f'PCMIRROR|IPAD_FILL|{fill}'
                before=sum(acknowledgement in line for line in output)
                process.stdin.write(f'IPAD_FILL {fill}\n');process.stdin.flush()
                deadline=time.monotonic()+5
                while sum(acknowledgement in line for line in output)==before:
                    pump();assert time.monotonic()<deadline,output;time.sleep(.02)
                wait_view(bool(fill) and model.startswith('iPad'))
            assert u.SetWindowPos(parent,None,0,0,600,800,0x14)
            wait_view(model.startswith('iPad'))
            process.stdin.write('IPAD_FILL 0\n');process.stdin.flush();wait_view(False)
            process.stdin.write('STOP\n');process.stdin.flush()
            assert process.wait(timeout=5)==0
            thread.join(timeout=1)
            print('PASS actual native viewport, live toggle and resize:',model)
        finally:
            if process.poll() is None:process.kill();process.wait(timeout=5)
finally:
    u.DestroyWindow(parent)

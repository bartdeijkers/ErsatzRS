#!/usr/bin/env python3
"""Observe a verified owned X11 window's client origin; Linux x64 proof only."""
import platform
import sys
if sys.platform != "linux" or platform.machine() != "x86_64":
    raise SystemExit("This receipted native proof helper requires Linux x64/X11")
import ctypes as C,sys,json
x=C.CDLL('libX11.so.6');x.XOpenDisplay.argtypes=[C.c_char_p];x.XOpenDisplay.restype=C.c_void_p;d=x.XOpenDisplay(None)
x.XDefaultRootWindow.argtypes=[C.c_void_p];x.XDefaultRootWindow.restype=C.c_ulong;r=x.XDefaultRootWindow(d)
x.XTranslateCoordinates.argtypes=[C.c_void_p,C.c_ulong,C.c_ulong,C.c_int,C.c_int,C.POINTER(C.c_int),C.POINTER(C.c_int),C.POINTER(C.c_ulong)]
a=C.c_int();b=C.c_int();c=C.c_ulong();x.XTranslateCoordinates(d,int(sys.argv[1]),r,0,0,C.byref(a),C.byref(b),C.byref(c));print(json.dumps({'x':a.value,'y':b.value}))

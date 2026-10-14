#!/usr/bin/env python3
"""Manual Linux x64/X11 input for owned synthetic teletext client windows.

The caller must verify _NET_WM_PID before any control. This drives actual Qt
widgets and never reads or alters native teletext data or decoder state.
"""
import platform
import sys
if sys.platform != "linux" or platform.machine() != "x86_64":
    raise SystemExit("This receipted native proof helper requires Linux x64/X11")
import ctypes as C, sys, time, json
x=C.CDLL('libX11.so.6'); xt=C.CDLL('libXtst.so.6')
x.XOpenDisplay.argtypes=[C.c_char_p]; x.XOpenDisplay.restype=C.c_void_p
d=x.XOpenDisplay(None)
if not d:raise SystemExit('display unavailable')
x.XDefaultRootWindow.argtypes=[C.c_void_p]; x.XDefaultRootWindow.restype=C.c_ulong
root=x.XDefaultRootWindow(d)
x.XQueryTree.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_ulong),C.POINTER(C.c_ulong),C.POINTER(C.POINTER(C.c_ulong)),C.POINTER(C.c_uint)]
x.XFetchName.argtypes=[C.c_void_p,C.c_ulong,C.POINTER(C.c_char_p)]
x.XFree.argtypes=[C.c_void_p];x.XFlush.argtypes=[C.c_void_p]
x.XSetInputFocus.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_ulong]
x.XRaiseWindow.argtypes=[C.c_void_p,C.c_ulong]
x.XMoveResizeWindow.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_int,C.c_uint,C.c_uint]
x.XStringToKeysym.argtypes=[C.c_char_p];x.XStringToKeysym.restype=C.c_ulong
x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong];x.XKeysymToKeycode.restype=C.c_uint
xt.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
xt.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]
xt.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
class Button(C.Structure):
 _fields_=[('type',C.c_int),('serial',C.c_ulong),('send_event',C.c_int),('display',C.c_void_p),('window',C.c_ulong),('root',C.c_ulong),('subwindow',C.c_ulong),('time',C.c_ulong),('x',C.c_int),('y',C.c_int),('x_root',C.c_int),('y_root',C.c_int),('state',C.c_uint),('button',C.c_uint),('same_screen',C.c_int)]
class Event(C.Union):
 _fields_=[('button',Button),('pad',C.c_long*24)]
x.XSendEvent.argtypes=[C.c_void_p,C.c_ulong,C.c_int,C.c_long,C.POINTER(Event)]
x.XSendEvent.restype=C.c_int
assert C.sizeof(Event)==192 and C.sizeof(C.c_long)==8, 'Linux x64 proof only'
def key(name,down):xt.XTestFakeKeyEvent(d,x.XKeysymToKeycode(d,x.XStringToKeysym(name.encode())),down,0)
def tree(w):
 r=C.c_ulong();p=C.c_ulong();cs=C.POINTER(C.c_ulong)();n=C.c_uint()
 x.XQueryTree(d,w,C.byref(r),C.byref(p),C.byref(cs),C.byref(n))
 children=[cs[i] for i in range(n.value)]
 if cs:x.XFree(cs)
 for c in children:
  name=C.c_char_p();x.XFetchName(d,c,C.byref(name))
  if name.value:
   title=name.value.decode(errors='replace')
   if 'VLC media player' in title:print(json.dumps({'window':c,'title':title}))
   x.XFree(name)
  tree(c)
if sys.argv[1]=='list':tree(root)
elif sys.argv[1]=='focus':
 w=int(sys.argv[2]);x.XRaiseWindow(d,w);x.XSetInputFocus(d,w,1,0)
elif sys.argv[1]=='move':x.XMoveResizeWindow(d,int(sys.argv[2]),*[int(v) for v in sys.argv[3:7]])
elif sys.argv[1]=='click-window':
 w,px,py,gx,gy=map(int,sys.argv[2:7])
 assert w>0 and 0<=px<600 and 0<=py<440
 for typ,mask,state in [(4,4,0),(5,8,256)]:
  event=Event();event.button=Button(typ,0,1,d,w,root,0,0,px,py,gx,gy,state,1,1)
  assert x.XSendEvent(d,w,1,mask,C.byref(event)), 'Owned-window button delivery failed'
  x.XFlush(d);time.sleep(.1)
elif sys.argv[1]=='click':
 xt.XTestFakeMotionEvent(d,-1,int(sys.argv[2]),int(sys.argv[3]),0)
 xt.XTestFakeButtonEvent(d,1,1,0);xt.XTestFakeButtonEvent(d,1,0,0)
elif sys.argv[1]=='type':
 for c in sys.argv[2]:key(c,1);key(c,0)
elif sys.argv[1]=='key':
 for k in sys.argv[2].split('+'):key(k,1)
 for k in reversed(sys.argv[2].split('+')):key(k,0)
x.XFlush(d)

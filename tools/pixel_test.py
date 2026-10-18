"""ADB transport for the Pixel project's existing native Linux tools."""
import os
import json
from pathlib import Path
import shlex
import subprocess
import sys

ADB = str(Path(os.environ['LOCALAPPDATA']) / 'Android/Sdk/platform-tools/adb.exe')
ENTRY = '/data/local/pixel-linux/bin/pixel-linux-entry'
ROOT = '/data/local/pixel-linux/rootfs'
SERIAL = os.environ.get('PIXEL_ADB_SERIAL')
if not SERIAL:
    devices = subprocess.check_output([ADB, 'devices'], text=True).splitlines()[1:]
    connected = [line.split()[0] for line in devices if len(line.split()) == 2 and line.split()[1] == 'device']
    if len(connected) != 1:
        raise SystemExit('Connect one Pixel, or set PIXEL_ADB_SERIAL to select it.')
    SERIAL = connected[0]

def android(command):
    return subprocess.run([ADB, '-s', SERIAL, 'shell', 'su -c ' + shlex.quote(command)], check=True)

if __name__ == '__main__':
    mode, *args = sys.argv[1:]
    if mode == 'exec':
        android(ENTRY + ' /bin/bash -lc ' + shlex.quote(args[0]))
    elif mode == 'script':
        android(ENTRY + ' /bin/bash -lc ' + shlex.quote(Path(args[0]).read_text(encoding='utf-8')))
    elif mode == 'desktop':
        data = json.loads(Path(args[0]).read_text()) if Path(args[0]).is_file() else {'action': args[0]}
        if args[0] in ('click', 'move', 'scroll'):
            data.update(x=int(args[1]), y=int(args[2]))
            if args[0] == 'click' and len(args)>3: data['clicks'] = int(args[3])
            if args[0] == 'scroll': data.update(direction=args[3], amount=int(args[4]))
        if args[0] == 'key': data['key'] = args[1]
        if args[0] == 'type': data['text'] = args[1]
        android(ENTRY + ' pixel-desktop-tool ' + shlex.quote(json.dumps(data)))
    elif mode == 'pull':
        android('cp ' + shlex.quote(ROOT + args[0]) + ' /data/local/tmp/cad-transfer')
        subprocess.run([ADB, '-s', SERIAL, 'pull', '/data/local/tmp/cad-transfer', args[1]], check=True)
    elif mode == 'push':
        subprocess.run([ADB, '-s', SERIAL, 'push', args[0], '/data/local/tmp/cad-transfer-in'], check=True)
        android('cp /data/local/tmp/cad-transfer-in ' + shlex.quote(ROOT + args[1]))
    else:
        raise SystemExit('Use exec, script, desktop, pull, or push')

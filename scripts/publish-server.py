"""Publish validated Velopack files over SFTP; expose the new feed last.

Uses an existing OpenSSH deployment key, never a password or root account.
Local connection settings live in .tools/ssh/publish.json, outside source bundles.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import urllib.request

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--release-directory',type=Path,default=root/'dist/releases')
parser.add_argument('--config',type=Path,default=root/'.tools/ssh/publish.json')
args = parser.parse_args()
folder = args.release_directory.resolve()
baseline=root/'deployment/server/runtime-sources.json'
runtime=root/'build/package/runtime-sources.json'
if baseline.exists() and runtime.exists() and json.loads(baseline.read_text())!=json.loads(runtime.read_text()):
    raise SystemExit('Runtime dependencies changed: publish matching runtime sources and update the source reference before releasing')
config = json.loads(args.config.read_text(encoding='utf-8-sig'))
remote = config['directory']
if not re.fullmatch(r'/[A-Za-z0-9_./-]+',remote) or '..' in remote.split('/'):
    raise SystemExit('Invalid remote release folder')
feed_path = folder/'releases.win.json'
feed = json.loads(feed_path.read_text())
assets = feed.get('Assets',[])
if not assets:
    raise SystemExit('Empty release feed')
def valid_name(name):
    if not re.fullmatch(r'[A-Za-z0-9_.-]+',name) or name.startswith('.'):
        raise SystemExit('Invalid release file name: '+name)
    return name
for asset in assets:
    name=valid_name(asset['FileName']);path=folder/name
    if not re.fullmatch(r'[a-fA-F0-9]{64}',asset.get('SHA256','')):
        raise SystemExit('Missing SHA256: '+name)
    if path.exists():
        with path.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
        if path.stat().st_size != asset['Size'] or digest.lower()!=asset['SHA256'].lower():
            raise SystemExit('Release hash/size mismatch: '+name)
    else:
        request=urllib.request.Request(config['url'].rstrip('/')+'/'+name,method='HEAD')
        with urllib.request.urlopen(request,timeout=30) as response:
            if int(response.headers['Content-Length'])!=asset['Size']:
                raise SystemExit('Remote base package size mismatch: '+name)
index=json.loads((folder/'assets.win.json').read_text())
names={valid_name(item['RelativeFileName']) for item in index}
versions=[a['Version'] for a in assets if a['Type']=='Full']
latest=max(versions,key=lambda v:tuple(int(p) for p in v.split('.')))
names.add(f'PC-Mirror-{latest}-source.zip')
if (folder/'SOURCES.txt').exists():names.add('SOURCES.txt')
files=[folder/name for name in sorted(names)]
if any(not p.is_file() for p in files):raise SystemExit('Latest release asset is missing')
for path in files:valid_name(path.name)
def quoted(value):
    value=str(value).replace('\\','/')
    if any(c in value for c in ('"','\r','\n')):raise ValueError('Invalid SFTP path')
    return '"'+value+'"'
commands=['cd '+quoted(remote)]
for path in files:
    temporary='.'+path.name+'.partial'
    commands.extend(['put '+quoted(path)+' '+quoted(temporary),'chmod 644 '+quoted(temporary),'rename '+quoted(temporary)+' '+quoted(path.name)])
for name in ('assets.win.json','RELEASES','releases.win.json'):
    path=folder/name
    if path.exists():
        commands.extend(['put '+quoted(path)+' '+quoted('.'+name+'.partial'),'chmod 644 '+quoted('.'+name+'.partial'),'rename '+quoted('.'+name+'.partial')+' '+quoted(name)])
key=Path(config['key']).expanduser();known=Path(config['known_hosts']).expanduser()
sftp=str(Path(os.environ.get('WINDIR','C:/Windows'))/'System32/OpenSSH/sftp.exe') if os.name=='nt' else 'sftp'
with tempfile.NamedTemporaryFile('w',encoding='utf-8',suffix='.sftp',delete=False) as batch:
    batch.write('\n'.join(commands)+'\n');batch_path=Path(batch.name)
try:
    print(f'Publishing {len(files)} files to {config["url"]}; feed last',flush=True)
    subprocess.run([sftp,'-b',str(batch_path),'-P',str(config.get('port',22)),'-i',str(key),'-o','IdentitiesOnly=yes',
        '-o','BatchMode=yes','-o','StrictHostKeyChecking=yes','-o','UserKnownHostsFile='+str(known),config['user']+'@'+config['host']],check=True)
finally:
    batch_path.unlink(missing_ok=True)
request=urllib.request.Request(config['url'].rstrip('/')+'/releases.win.json',headers={'Cache-Control':'no-cache'})
with urllib.request.urlopen(request,timeout=30) as response:
    public=json.load(response)
    if public!=feed:raise SystemExit('Public update feed does not match uploaded release')
    if 'no-store' not in response.headers.get('Cache-Control',''):
        raise SystemExit('Update feed must not be cached')
print('PASS public HTTPS release feed matches local package feed',flush=True)

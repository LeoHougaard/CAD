"""Exact public request/reply bytes for the focused host acceptance drivers."""
import json, subprocess, time
from pathlib import Path
from preview_workflows import PreviewSession
class Tee:
    def __init__(self, stream, path): self.stream=stream;self.path=path
    def write(self,text):
        with self.path.open('a',encoding='utf-8',newline='') as f:f.write(text)
        return self.stream.write(text)
    def readline(self):
        text=self.stream.readline()
        with self.path.open('a',encoding='utf-8',newline='') as f:f.write(text)
        return text
    def __getattr__(self,name):return getattr(self.stream,name)
class RecordedSession(PreviewSession):
    def __init__(self,host,runtime,output,prefix):
        command=[str(host),'--executable',runtime['executable'],'--script',runtime['script']]
        for arg in runtime['args']:command.extend(['--arg',arg])
        self.process=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,encoding='utf-8',creationflags=getattr(subprocess,'BELOW_NORMAL_PRIORITY_CLASS',0))
        self.process.stdin=Tee(self.process.stdin,output/(prefix+'-requests.ndjson'))
        self.process.stdout=Tee(self.process.stdout,output/(prefix+'-replies.ndjson'))
        self.timings=[];self.revisions={};start=time.perf_counter()
        self.capabilities=self.call('capabilities');self.startup_ms=(time.perf_counter()-start)*1000

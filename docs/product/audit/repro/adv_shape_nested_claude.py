import json,glob,os,re
n=0; files=set()
pat=re.compile(r'(^|[;&|(\s])claude\s+(-p|--print)\b')
for path in glob.glob(os.path.expanduser('~/.claude/projects/*/*.jsonl')):
    for l in open(path,'rb'):
        if b'claude -p' not in l and b'claude --print' not in l: continue
        try: v=json.loads(l)
        except Exception: continue
        if v.get('type')!='assistant': continue
        for b in (v.get('message') or {}).get('content') or []:
            if isinstance(b,dict) and b.get('type')=='tool_use' and b.get('name')=='Bash':
                cmd=(b.get('input') or {}).get('command','')
                if pat.search(cmd): n+=1; files.add(os.path.basename(path)[:8])
print('Bash tool_use commands running claude -p/--print:', n, 'in', len(files), 'sessions')

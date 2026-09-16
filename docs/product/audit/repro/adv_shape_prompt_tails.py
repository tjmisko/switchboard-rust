import json,collections,glob,os
PRE=("<local-command","<command-","<bash-","<user-memory-input")
out=collections.Counter()
for path in sorted(glob.glob(os.path.expanduser('~/.claude/projects/*/*.jsonl'))):
    last_prompt=None; tail=[]
    for l in open(path,'rb'):
        if len(l)>(1<<20): continue
        try: v=json.loads(l)
        except Exception: continue
        if v.get('isSidechain') or v.get('isMeta'): continue
        t=v.get('type'); m=v.get('message') or {}; c=m.get('content')
        if t=='user' and isinstance(c,str) and not c.startswith(PRE) and 'interruptedMessageId' not in v:
            last_prompt=True; tail=[]
            continue
        if last_prompt is None: continue
        if t=='assistant':
            last_prompt=None; tail=[]; continue
        if t=='user':
            desc='user:'+('marker' if isinstance(c,list) and any(isinstance(b.get('text'),str) and b['text'].startswith('[Request interrupted') for b in c) else ('imid' if 'interruptedMessageId' in v else ('str' if isinstance(c,str) else 'arr')))
        else:
            desc=t+(':'+str(v.get('subtype')) if v.get('subtype') else '')
        tail.append(desc)
    if last_prompt:
        out[tuple(dict.fromkeys(tail))[:4]]+=1
for k,v in out.most_common(): print(v,k)

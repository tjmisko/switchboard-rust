import json,collections,glob,os
c1=collections.Counter()
big=collections.Counter()
for path in sorted(glob.glob(os.path.expanduser('~/.claude/projects/*/*.jsonl'))):
    for l in open(path,'rb'):
        if len(l) > (1<<20):
            try:
                v=json.loads(l); m=v.get('message') or {}; c=m.get('content')
                big[(v.get('type'), tuple(b.get('type') for b in c) if isinstance(c,list) else type(c).__name__, len(l)//(1<<20))]+=1
            except Exception as e: big[('unparsable',)]+=1
            continue
        try: v=json.loads(l)
        except Exception: continue
        if v.get('type')!='user' or v.get('isSidechain') or v.get('isMeta') or 'interruptedMessageId' in v: continue
        c=(v.get('message') or {}).get('content')
        if not isinstance(c,list): continue
        types=tuple(b.get('type') for b in c)
        if 'tool_result' in types: continue
        texts=[b.get('text','') for b in c if b.get('type')=='text']
        if any(isinstance(t,str) and t.startswith('[Request interrupted') for t in texts): continue
        prefix=[t[:18] if isinstance(t,str) and t.startswith('<') else '<prose>' for t in texts]
        c1[(types, (v.get('origin') or {}).get('kind'), tuple(prefix), v.get('version','')[:6])]+=1
for k,v in c1.most_common(20): print(v,k)
print('--- lines > 1MiB')
for k,v in big.most_common(): print(v,k)

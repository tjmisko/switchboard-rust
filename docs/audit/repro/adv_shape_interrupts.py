import json,collections,glob,os
ctx=collections.Counter(); after=collections.Counter()
for path in sorted(glob.glob(os.path.expanduser('~/.claude/projects/*/*.jsonl'))):
    rows=[]
    for l in open(path,'rb'):
        try: v=json.loads(l)
        except Exception: continue
        if v.get('type') not in ('user','assistant'): continue
        m=v.get('message') or {}
        c=m.get('content')
        blocks=[b.get('type') for b in c] if isinstance(c,list) else ('str' if isinstance(c,str) else None)
        marker=''
        if isinstance(c,list):
            for b in c:
                t=b.get('text')
                if isinstance(t,str) and t.startswith('[Request interrupted'): marker=t[:45]
        elif isinstance(c,str) and c.startswith('[Request interrupted'): marker=c[:45]
        errs=tuple(b.get('is_error') for b in c if isinstance(c,list) and b.get('type')=='tool_result') if isinstance(c,list) else ()
        rows.append(dict(t=v['type'],imid='interruptedMessageId' in v,blocks=tuple(blocks) if isinstance(blocks,list) else blocks,sr=m.get('stop_reason'),err=errs,marker=marker,side=v.get('isSidechain'),meta=v.get('isMeta')))
    for i,r in enumerate(rows):
        if r['marker']:
            p=rows[i-1] if i>0 else {}
            n=rows[i+1] if i+1<len(rows) else {}
            ctx[(r['marker'],r['imid'],r['blocks'],p.get('t'),p.get('blocks'),p.get('sr'),p.get('err'))]+=1
for k,v in ctx.most_common(): print(v,k)

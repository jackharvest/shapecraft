# make_art.py SCRATCH_DIR [tut] [new] — builds the home screen's pictures (assets/home/*.png) from our own models with
# shapecraft-cli (see-through PNGs, 680x876 = 2x the 340x438 card art). new-cards.png is a crop of our own Fillet card
# rendered by shapecraft-snap at "scale": 2; new-shapr.png renders a .shapr file we made ourselves (set SHAPR below).
import json, subprocess, sys, os
CLI=os.environ.get('SHAPECRAFT_CLI', os.path.expanduser('~/.cache/shapecraft-target/release/shapecraft-cli'))
OUT=os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
SHAPR=os.environ.get('SHAPR_SAMPLE', '')
S=sys.argv[1]
def run(name, cmds, size=(680,876), png=True):
    path=f'{S}/{name}.jsonl'
    open(path,'w').write('\n'.join(json.dumps({'command':c,'params':p}) for c,p in cmds)+'\n')
    args=[CLI,'run',path]+(['--png',f'{OUT}/{name}.png','--size',f'{size[0]}x{size[1]}','--clear'] if png else [])
    r=subprocess.run(args,capture_output=True,text=True)
    if r.returncode: print(name, r.stdout[-400:], r.stderr[-800:]); sys.exit(1)
    return r.stdout
def regions(cmds):
    out=run('_probe', cmds+[('inspect',{})], png=False)
    d=json.loads([l for l in out.splitlines() if l.startswith('inspect: ')][-1][9:])
    return [r['region'] for r in d['sketches'][-1]['regions']]

stand=[('sketch',{'plane':'xy'}),('rectangle',{'center':[0,0],'size':[90,70]}),('exit_sketching',{}),
 ('extrude',{'sketch':1,'region':0,'distance':8}),
 ('sketch',{'origin':[0,0,8],'normal':[0,0,1]}),('rectangle',{'center':[-28,0],'size':[14,90]}),('exit_sketching',{}),
 ('extrude',{'sketch':3,'region':0,'distance':70,'result':'union'}),
 ('sketch',{'origin':[0,0,8],'normal':[0,0,1]}),('rectangle',{'center':[12,0],'size':[6,90]}),('exit_sketching',{}),
 ('extrude',{'sketch':4,'region':0,'distance':-4,'result':'subtract'}),
 ('select',{'picks':[{'kind':'body','body':2}]}),('fillet',{'radius':1.5}),('deselect_all',{})]
cup=[('sketch',{'plane':'xy'}),('circle',{'center':[0,0],'radius':35}),('exit_sketching',{}),
 ('extrude',{'sketch':1,'region':0,'distance':95}),('shell',{'body':2,'faces':[2],'thickness':3}),
 ('select',{'picks':[{'kind':'body','body':2}]}),('fillet',{'radius':1}),('deselect_all',{})]
tag=[('sketch',{'plane':'xy'}),('rectangle',{'center':[0,0],'size':[60,30]}),('exit_sketching',{}),
 ('extrude',{'sketch':1,'region':0,'distance':4}),
 ('sketch',{'origin':[0,0,4],'normal':[0,0,1]}),('circle',{'center':[0,-22],'radius':4}),('exit_sketching',{}),
 ('extrude',{'sketch':3,'region':0,'distance':-5,'result':'subtract'}),
 ('select',{'picks':[{'kind':'body','body':2}]}),('fillet',{'radius':1}),('deselect_all',{})]
which=sys.argv[2:] or ['all']
def want(n): return 'all' in which or n in which
if want('tut'):
    run('tut-phone-stand', stand)
    run('tut-pencil-cup', cup)
    run('tut-key-tag', tag)
if want('new'):
    run('new-tutorials', stand+[('set_appearance',{'items':[2],'appearance':{'name':'Teal','color':[0.16,0.55,0.62],'metal':0,'gloss':0.5}})])
    cube=[('sketch',{'plane':'xy'}),('rectangle',{'center':[0,0],'size':[40,40]}),('exit_sketching',{}),
      ('extrude',{'sketch':1,'region':0,'distance':40}),('select',{'picks':[{'kind':'body','body':2}]}),('chamfer',{'distance':4}),('deselect_all',{})]
    run('new-view-cube', cube)
    copies=[('sketch',{'plane':'xy'}),('rectangle',{'center':[0,0],'size':[24,24]}),('exit_sketching',{}),
      ('extrude',{'sketch':1,'region':0,'distance':6}),
      ('sketch',{'origin':[0,0,6],'normal':[0,0,1]}),('circle',{'center':[0,0],'radius':6}),('exit_sketching',{}),
      ('extrude',{'sketch':3,'region':0,'distance':14,'result':'union'}),
      ('select',{'picks':[{'kind':'body','body':2}]}),('fillet',{'radius':1}),
      ('translate',{'by':[0,34,0],'items':[2],'copy':True,'copies':1}),('select_all',{}),
      ('translate',{'by':[34,0,0],'copy':True,'copies':1}),('deselect_all',{})]
    run('new-copies', copies)
    plate=[('sketch',{'plane':'xy'}),('rectangle',{'center':[0,0],'size':[40,84]}),('exit_sketching',{}),
      ('extrude',{'sketch':1,'region':0,'distance':4}),
      ('sketch',{'body':2,'face':5}),('text',{'at':[8,-26],'text':'Hola','size':22,'angle':90})]
    rs=regions(plate)
    run('new-languages', plate+[('exit_sketching',{}),('extrude',{'sketch':3,'regions':rs,'distance':2.5,'result':'new'}),
      ('select',{'picks':[{'kind':'body','body':2}]}),('fillet',{'radius':1.2}),('deselect_all',{}),
      ('set_appearance',{'items':[2],'appearance':{'name':'Plate','color':[0.85,0.85,0.86],'metal':0,'gloss':0.4}})])
    if SHAPR:
        run('new-shapr', [('import_file',{'path':SHAPR})])

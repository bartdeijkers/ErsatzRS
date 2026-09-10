#!/usr/bin/env python3
"""Local synthetic UX renderer. No ErsatzRS access, JS, disk persistence or provider calls.
Run: python3 plans/admin-ux-organize/preview_server.py --port 8412
State expires with this process. This is not a production API or numbering implementation.
"""
import argparse, copy, json, re, secrets, time
from datetime import date, datetime, timezone
from email.parser import BytesParser
from email.policy import default
from http.cookies import SimpleCookie
from http.server import HTTPServer, SimpleHTTPRequestHandler
from pathlib import Path
from urllib.parse import parse_qs, urlsplit
from html_tree import Node, parse, fragment

ROOT=Path(__file__).resolve().parent
SESSIONS={}
RUNTIMES={1:108,2:2538,3:2587,4:1690,5:125,6:2652,7:2706,8:384,9:None}
DATES={1:'2025-02-01',2:'2025-02-08',3:'2025-02-15',4:'2025-12-24',5:'2026-01-10',6:'2026-04-04',7:'2026-04-11',8:'2026-04-12',9:'2026-04-18'}

def val(d,k,fallback=''):
    v=d.get(k,[]);return v[-1] if v else fallback

def values(form):
    out={}
    for n in form.walk():
        k=n.attrs.get('name')
        if not k or n.tag not in ['input','select','textarea'] or 'disabled' in n.attrs:continue
        if n.tag=='input':
            if n.attrs.get('type') in ['checkbox','radio'] and 'checked' not in n.attrs:continue
            value=n.attrs.get('value','')
        elif n.tag=='textarea':value=n.text()
        else:
            choices=n.find(tag='option');picked=next((o for o in choices if 'selected' in o.attrs),choices[0] if choices else None)
            value=picked.attrs.get('value',picked.text().strip()) if picked else ''
        out.setdefault(k,[]).append(value)
    return out

def overlay(form,data):
    for n in form.walk():
        k=n.attrs.get('name')
        if k not in data or n.tag not in ['input','select','textarea']:continue
        if n.tag=='input':
            if n.attrs.get('type') in ['checkbox','radio']:
                n.attrs.pop('checked',None)
                if n.attrs.get('value','on') in data[k]:n.attrs['checked']=None
            elif n.attrs.get('type')!='file':n.attrs['value']=val(data,k)
        elif n.tag=='textarea':n.children=[val(data,k)]
        else:
            for o in n.find(tag='option'):
                o.attrs.pop('selected',None)
                if o.attrs.get('value',o.text().strip())==val(data,k):o.attrs['selected']=None

def initial():
    state={'drafts':{},'applied':{},'revision':29,'notice':'','error':False,'preview':None,'page':0,'long':False,'last':time.time(),'csrf':secrets.token_urlsafe(24)}
    for filename in ['index.html','settings.html','artwork.html','options.html']+[f'item-{i}.html' for i in range(1,10)]:
        dom=parse((ROOT/filename).read_text())
        for f in dom.find(tag='form'):
            g=f.attrs.get('data-draft','misc');d=values(f)
            for name in val(d,'checkbox_names').split(','):
                if name:d.setdefault(name,[])
            state['drafts'].setdefault(g,{}).update(d)
    state['applied']=copy.deepcopy(state['drafts']);return state

def integer(text,label,minimum=0,maximum=2147483647):
    if not re.fullmatch(r'\d+',text):raise ValueError(label+' needs a whole number.')
    n=int(text)
    if not minimum<=n<=maximum:raise ValueError(label+' is outside the allowed range.')
    return n

def duration(text):
    if not text:return None
    if not re.fullmatch(r'\d+:[0-5]\d',text):raise ValueError('Enter length as minutes:seconds, for example 42:30.')
    a,b=map(int,text.split(':'));total=a*60+b
    if total>2147483647:raise ValueError('Length is too large.')
    return total

def pair(d,i):return val(d,f'items[{i}].season'),val(d,f'items[{i}].episode')
def setpair(d,i,p):
    d[f'items[{i}].season']=[str(p[0])];d[f'items[{i}].episode']=[str(p[1])]
def fmt(p):return 'Unnumbered' if not p[0] and not p[1] else f'S{p[0]} · E{p[1]}'
def ids(state):return list(range(1,166 if state.get('large') else 37 if state['long'] else 10))
def list_file(state):return ROOT/('large-list.html' if state.get('large') else 'long-list.html' if state['long'] else 'index.html')
def runtime_for(state,i):return None if state.get('large') else RUNTIMES.get(i,2600 if i>9 else None)

class ItemMetadataError(ValueError):
    pass

def validate_chapters(text,runtime):
    previous=-1
    for line,raw in enumerate(text.splitlines(),1):
        if not raw.strip():continue
        match=re.fullmatch(r'(\d+:[0-5]\d|\d+:[0-5]\d:[0-5]\d)\s+(.+)',raw.strip())
        if not match:raise ItemMetadataError(f'Chapters, line {line}: use a time followed by a title, for example 0:00 Introduction.')
        parts=list(map(int,match[1].split(':')));seconds=0
        for part in parts:seconds=seconds*60+part
        if seconds<=previous:raise ItemMetadataError(f'Chapters, line {line}: times must increase without duplicates.')
        if runtime is not None and seconds>=runtime:raise ItemMetadataError(f'Chapters, line {line}: the chapter must start before the item ends.')
        previous=seconds

def eligible_rows(state):
    d=state['drafts']['items'];items=ids(state);category=val(d,'review_category');q=val(d,'item_query').lower()
    if category.startswith('Unnumbered'):items=[i for i in items if pair(d,i)==('','')]
    elif category.startswith('Needs review'):items=[i for i in items if state.get('large') or state['long'] and i in [10,12]]
    elif category.startswith('Excluded'):items=[4] if state['long'] and not state.get('large') else []
    elif category.startswith('Unavailable'):items=[10] if state['long'] and not state.get('large') else []
    elif category.startswith('Conflicted'):items=[12] if state['long'] and not state.get('large') else []
    elif category and not category.startswith('All'):items=[]
    if q:
        dom=parse((list_file(state)).read_text())
        titles={int(r.attrs['id'].split('-')[-1]):r.one(cls='episode-identity').text().lower() for r in dom.find(cls='episode-row')}
        for i in items:
            titles[i]=val(state['applied'].get(f'item-{i}',{}),'title',titles.get(i,'')).lower()
        items=[i for i in items if q in titles.get(i,'')]
    return items

def page_ids(state):
    d=state['drafts']['items'];all_ids=eligible_rows(state);size=val(d,'page_size','10');size=len(all_ids) or 1 if size=='All' else max(1,min(100,int(size)))
    state['page']=min(state['page'],max(0,(len(all_ids)-1)//size))
    return all_ids[state['page']*size:(state['page']+1)*size]

def targets(state):
    d=state['drafts']['items'];scope=val(d,'bulk_scope','checked')
    return {'all':ids(state),'filter':eligible_rows(state),'page':page_ids(state)}.get(scope,[i for i in ids(state) if str(i) in d.get('selected_ids',[])])

def preview(state,anchor=None):
    d=state['drafts']['items'];applied=state['applied']['items'];candidate=targets(state)
    prefix='selected';start=val(d,'selected_season','2');first=val(d,'selected_first_episode','1');replace=True;count=None;detect=False
    if anchor is not None:
        end=integer(val(d,f'range[{anchor}].end_id',str(anchor)),'Range endpoint',anchor,max(ids(state)))
        candidate=list(range(anchor,end+1));prefix=f'range-{anchor}';start=val(d,f'range[{anchor}].season','2');first=val(d,f'range[{anchor}].first_episode','1')
        replace=val(d,f'range[{anchor}].replace_existing')=='true';detect=val(d,f'range[{anchor}].detect_dates')=='true'
        text=val(d,f'range[{anchor}].count');count=integer(text,'Maximum assignments',1) if text else None
    if not candidate:raise ValueError('Choose at least one item or an explicit bulk scope.')
    low=duration(val(d,prefix+'.minimum_length'));high=duration(val(d,prefix+'.maximum_length'))
    if low is not None and high is not None and low>high:raise ValueError('Minimum length must not exceed maximum length.')
    season=integer(start,'Season');episode=integer(first,'First episode',1);unknown=val(d,prefix+'.include_unknown_length')=='true'
    reasons={};accepted=[]
    for i in candidate:
        runtime=runtime_for(state,i)
        reason=''
        if state['long'] and not state.get('large') and i==4:reason='Excluded from publication'
        elif state['long'] and not state.get('large') and i==11:reason='Live source'
        elif (low is not None or high is not None) and runtime is None and not unknown:reason='Unknown length'
        elif runtime is not None and low is not None and runtime<low:reason='Shorter than minimum'
        elif runtime is not None and high is not None and runtime>high:reason='Longer than maximum'
        elif not replace and not detect and pair(applied,i)!=('',''):reason='Existing numbers retained'
        elif count is not None and len(accepted)>=count:reason='Assignment limit'
        if reason:reasons[i]=reason
        else:accepted.append(i)
    occupied={pair(applied,i) for i in ids(state) if i not in accepted and pair(applied,i)!=('','')}
    changes={};last_date=None;gap=integer(val(d,f'range[{anchor}].gap_days','90'),'Gap days',1) if detect else None
    if detect:episode=1
    for i in accepted:
        current_date=date.fromisoformat(DATES[i]) if i in DATES and not state.get('large') else None
        if detect and last_date and current_date and (current_date-last_date).days>=gap:season+=1;episode=1
        if current_date:last_date=current_date
        while (str(season),str(episode)) in occupied:episode+=1
        if episode>2147483647 or season>2147483647:raise ValueError('Numbering exceeds the allowed range.')
        proposed=(str(season),str(episode));changes[i]=proposed;occupied.add(proposed);episode+=1
    state['preview']={'changes':changes,'reasons':reasons,'candidate':candidate,'revision':state['revision'],'origin':f'numbering-{anchor}.html#episode-{anchor}' if anchor else 'numbering.html#group-actions'}

class NumberingError(ValueError):
    def __init__(self, message, item_ids):
        super().__init__(message)
        self.item_ids = item_ids

def notice(state,text,error=False):
    state['notice']=text;state['error']=error
    state.pop('numbering_error_ids',None)
    state.pop('focus_error',None)
    state.pop('item_error_field',None)
    state.pop('error_group',None)

def act(state,data,origin):
    group=val(data,'draft_group','misc');intent=val(data,'intent','apply');d=state['drafts'].setdefault(group,{})
    # Page selection is merged, not replaced by the absence of off-page checkboxes.
    old_selected=d.get('selected_ids',[]);visible=val(data,'visible_ids').split(',')
    for key in val(data,'checkbox_names').split(','):
        if key and key not in data:data[key]=[]
    if group=='items' and visible and 'selected_ids' in data:
        data['selected_ids']=list(dict.fromkeys([x for x in old_selected if x not in visible]+data['selected_ids']))
    for k,v in data.items():
        if k not in ['csrf','intent','origin','visible_ids','checkbox_names','draft_group']:d[k]=v
    if re.fullmatch(r'item-\d+',group):
        p=(val(d,'season'),val(d,'episode')) if val(d,'numbering_kind')=='numbered' else ('','')
        setpair(state['drafts']['items'],int(group[5:]),p)
    if intent.startswith('focus-numbering:'):
        item=integer(intent.split(':')[1],'Item',1,max(ids(state)))
        if item not in state.get('numbering_error_ids',[]):raise ValueError('This numbering error is no longer current.')
        # Keep drafts and selection while revealing a conflict outside the current page/filter.
        d['review_category']=['All items'];d['item_query']=['']
        size=val(d,'page_size','10')
        state['page']=0 if size=='All' else ids(state).index(item)//int(size)
        return f'index.html#numbering-{item}-episode'
    notice(state,'')
    if intent.startswith('nav:'):
        match=re.search(r'#episode-(\d+)$',intent)
        if match:
            order=eligible_rows(state);item=int(match[1]);size=val(state['drafts']['items'],'page_size','10')
            if item in order and size!='All':state['page']=order.index(item)//int(size)
        return intent[4:]
    if intent=='cancel-context':
        state.pop('context',None)
        return ('long-list.html' if state['long'] else 'numbering.html')+'#episode-'+str(state.get('anchor',6))
    if intent.startswith(('open-numbering:','open-selection:')):
        kind,n=intent.split(':');state['anchor']=int(n)
        if int(n)>9:state['context']=kind;return 'long-list.html#episode-'+n
        return ('numbering-' if kind=='open-numbering' else 'selection-')+n+'.html#episode-'+n
    if intent.startswith('select-range:'):
        n=int(intent.split(':')[-1]);end=integer(val(d,f'range[{n}].end_id',str(n)),'Endpoint',n,max(ids(state)))
        d['selected_ids']=list(map(str,range(n,end+1)));return ('long-list.html' if state['long'] else 'numbering.html')+'#episode-'+str(n)
    if intent=='clear-selection':d['selected_ids']=[];return origin+'#group-actions'
    if intent in ['filter','page-size']:state['page']=0;return origin+'#episode-editor'
    if intent in ['demo-retry-publication','demo-complete-publication']:
        expected='recovery' if intent=='demo-retry-publication' else 'running'
        if state.get('publication_demo')!=expected or val(data,'publication_generation')!=str(state.get('publication_generation',1)):
            raise ValueError('This publication state changed. Review the current recovery options; nothing was repeated.')
        state['publication_generation']=state.get('publication_generation',1)+1
        if expected=='recovery':
            state['publication_demo']='running';state['pending_revision']=state['revision']
            notice(state,'Synthetic retry started. No provider or production publication was called.')
        else:
            state['publication_demo']='complete';state['published_revision']=state['pending_revision'];state['published_snapshot']=4
            notice(state,'Synthetic publication completed. No production media was changed.')
        return 'index.html'
    if intent=='publish':
        if state.get('publication_demo') in ['recovery','running']:raise ValueError('Resolve the pending publication before publishing more changes.')
        state['published_revision']=state['revision'];state['published_snapshot']=state.get('published_snapshot',3)+1
        notice(state,'Saved changes published in this preview. Unsaved edits are retained; no production media was changed.')
        return origin
    if intent in ['next-page','previous-page']:
        state['page']=max(0,state['page']+(1 if intent=='next-page' else -1));return origin+'#episode-editor'
    if intent=='discard':
        state['drafts'][group]=copy.deepcopy(state['applied'].get(group,{}))
        if re.fullmatch(r'item-\d+',group):
            i=int(group[5:]);setpair(state['drafts']['items'],i,pair(state['applied']['items'],i))
        notice(state,'Pending changes discarded for this editing task.');return origin
    if intent=='undo-numbers':
        if 'undo' in state:
            prior=state.pop('undo')
            for i in ids(state):
                if pair(state['drafts']['items'],i)==pair(state['applied']['items'],i):setpair(state['drafts']['items'],i,pair(prior,i))
                setpair(state['applied']['items'],i,pair(prior,i))
            state['revision']+=1;notice(state,'Previous numbering restored; other pending edits and selection retained.')
        else:notice(state,'No numbering change to undo.')
        return origin
    if intent=='back-preview':return (state.get('preview') or {}).get('origin','numbering.html')
    if intent=='preview-selected-numbering' or intent.startswith('preview-range:'):
        preview(state,int(intent.split(':')[-1]) if ':' in intent else None);return 'numbering-preview.html'
    if intent=='apply-preview':
        p=state.get('preview')
        if not p:raise ValueError('Create a numbering preview from your current selection first.')
        if p['revision']!=state['revision']:raise ValueError('The list changed. Recalculate this preview; nothing was applied.')
        if not p['changes']:raise ValueError('No eligible items. Change the length bounds or selection.')
        state['undo']=copy.deepcopy(state['applied']['items'])
        for i,pair_value in p['changes'].items():setpair(state['applied']['items'],i,pair_value);setpair(state['drafts']['items'],i,pair_value)
        state['revision']+=1;notice(state,f"Saved {len(p['changes'])} numbering assignments. Publish from Items & review when ready.");return 'numbering.html#episode-editor'
    if intent=='preview-bulk':
        chosen=targets(state)
        if not chosen:raise ValueError('Choose items or an explicit bulk scope first.')
        state['bulk']={'ids':chosen,'revision':state['revision'],'kind':val(d,'bulk_kind'),'behavior':val(d,'bulk_behavior'),'cookies':val(d,'bulk_cookies'),'sponsor':val(d,'bulk_sponsor')};return 'bulk-preview.html'
    if intent=='apply-bulk':
        if not state.get('bulk'):raise ValueError('Preview a bulk change first.')
        if state['bulk']['revision']!=state['revision']:raise ValueError('The list changed. Preview this bulk change again.')
        state['bulk_undo']=copy.deepcopy(state.get('bulk_applied'))
        state['bulk_applied']=copy.deepcopy(state['bulk']);state['revision']+=1;notice(state,'Reviewed bulk values recorded in this synthetic preview; no production media was changed.');return 'numbering.html#group-actions'
    if intent=='undo-bulk':
        if 'bulk_undo' in state:
            state['bulk_applied']=state.pop('bulk_undo');state['revision']+=1;notice(state,'Previous bulk settings restored in this synthetic preview.')
        else:notice(state,'No bulk change to undo.')
        return origin+'#group-actions'
    if intent in ['apply-numbers','set-selected-season','clear-selected-numbers'] or intent.startswith('clear-row:'):
        chosen=ids(state) if intent=='apply-numbers' else targets(state)
        if intent.startswith('clear-row:'):chosen=[int(intent.split(':')[-1])]
        if not chosen:raise ValueError('Choose at least one item or an explicit bulk scope. No numbers changed.')
        updated=copy.deepcopy(state['applied']['items'])
        for i in chosen:
            s,e=pair(d,i)
            if intent=='set-selected-season':s=str(integer(val(d,'selected_season'),'Season'))
            elif intent=='clear-selected-numbers' or intent.startswith('clear-row:'):s=e=''
            if bool(s)!=bool(e):raise NumberingError('Enter both season and episode, or leave both blank. No changes applied.',[i])
            if s:
                try:integer(s,'Season');integer(e,'Episode',1)
                except ValueError as error:raise NumberingError(str(error)+' No changes applied.',[i]) from error
            setpair(updated,i,(s,e))
        seen={}
        for i in ids(state):
            p=pair(updated,i)
            if p==('',''):continue
            if p in seen:raise NumberingError('These items have the same season and episode. Give each a unique pair, or leave both fields blank. No changes applied.',[seen[p],i])
            seen[p]=i
        state['undo']=copy.deepcopy(state['applied']['items']);state['applied']['items']=updated
        for i in chosen:setpair(d,i,pair(updated,i))
        state['revision']+=1;notice(state,'Numbering saved. Publish from Items & review when you are ready.');return origin+'#episode-editor'
    if intent in ['reset-demo','restore-demo']:
        if val(d,'acknowledge')!='true':raise ValueError('Acknowledge the displayed impact before continuing.')
        if intent=='reset-demo' and val(d,'confirmed_name')!='The Screening Room':raise ValueError('Enter the displayed list title exactly.')
        notice(state,'Confirmation accepted for this demonstration. No list was reset or restored.');return origin
    if intent=='apply':
        if group=='playback':
            cookies=val(d,'cookie_policy','default');sponsor=val(d,'sponsor_policy','default')
            if cookies not in ['default','None','Firefox','Chrome','Chromium']:raise ValueError('Choose a browser access setting for this list.')
            if sponsor not in ['default','on','off']:raise ValueError('Choose a segment removal setting for this list.')
            try:warning=float(val(d,'warning_gb','25'))
            except ValueError:raise ValueError('Disk warning needs a nonnegative number.')
            if not 0<=warning<1e12:raise ValueError('Disk warning needs a nonnegative finite number.')
            # One policy choice preserves inheritance even when values happen to match.
            d['cookies_enabled']=['true'] if cookies in ['Firefox','Chrome','Chromium'] else []
            d['cookie_browser']=[cookies if d['cookies_enabled'] else 'Choose browser']
            d['sponsor_enabled']=['true'] if sponsor=='on' else []
            state['option_origins']={'cookies':'Using add-on default' if cookies=='default' else 'Set for this list','sponsor':'Using add-on default' if sponsor=='default' else 'Set for this list'}
            if 'new videos only' in val(d,'delivery_mode').lower() and not state.get('cutoff'):
                state['cutoff']=datetime.now(timezone.utc).strftime('%d %b %Y, %H:%M UTC')
        if group=='settings':
            if not val(d,'metadata_title').strip():raise ValueError('Enter a list title. Your other edits are retained.')
            for role in ['poster','fanart','thumbnail']:
                source=val(d,f'art_{role}_source','keep')
                if source=='url' and not val(d,'metadata_'+role).startswith(('https://','http://')):raise ValueError('Enter an HTTP(S) URL for '+role+'.')
                if source=='upload' and not val(d,'upload_'+role):raise ValueError('Choose a file for '+role+'.')
        if group.startswith('item-'):
            if not val(d,'title').strip():raise ValueError('Enter the item title.')
            if val(d,'numbering_kind')=='numbered':integer(val(d,'season'),'Season');integer(val(d,'episode'),'Episode',1)
            i=int(group[5:]);validate_chapters(val(d,'chapter_input'),runtime_for(state,i))
            p=(val(d,'season'),val(d,'episode')) if val(d,'numbering_kind')=='numbered' else ('','')
            if p!=('','') and any(pair(state['applied']['items'],other)==p for other in ids(state) if other!=i):raise ValueError('That season and episode belong to another item. Choose unused numbers; your edits are retained.')
            setpair(state['applied']['items'],i,p);setpair(state['drafts']['items'],i,p)
        state['applied'][group]=copy.deepcopy(d);state['revision']+=1;notice(state,'Changes saved. Publish from Items & review when you are ready.');return origin
    notice(state,'Synthetic example only; no provider call or production operation was performed.');return origin

class Handler(SimpleHTTPRequestHandler):
    def __init__(self,*a,**kw):super().__init__(*a,directory=str(ROOT),**kw)
    def log_message(self,*args):pass  # Never log submitted values or private upload names.
    def session(self):
        cookie=SimpleCookie();cookie.load(self.headers.get('Cookie',''));sid=cookie.get('ux_preview');sid=sid.value if sid else ''
        if sid not in SESSIONS:
            while len(SESSIONS)>=32:SESSIONS.pop(min(SESSIONS,key=lambda k:SESSIONS[k]['last']))
            sid=secrets.token_urlsafe(20);SESSIONS[sid]=initial()
        SESSIONS[sid]['last']=time.time();return sid,SESSIONS[sid]
    def do_GET(self):
        path=urlsplit(self.path).path.lstrip('/') or 'index.html';file=(ROOT/path).resolve()
        if not file.is_relative_to(ROOT):self.send_error(404);return
        virtual_item=re.fullmatch(r'item-(\d+)\.html',path)
        if virtual_item and 37<=int(virtual_item[1])<=165 and not file.exists():file=ROOT/'item-36.html'
        if file.suffix!='.html' or not file.exists():return super().do_GET()
        sid,state=self.session()
        if path in ['recovery-list.html','recovery.html'] and not state.get('publication_demo'):
            state['publication_demo']='recovery';state['publication_generation']=1
        if path in ['long-list.html','large-list.html'] or virtual_item and int(virtual_item[1])>36:
            state['long']=True
            if path=='large-list.html' or virtual_item and int(virtual_item[1])>36:state['large']=True
            f=parse(list_file(state).read_text()).one(id='workspace-form');base=values(f)
            if state.get('large') and not state.get('large_initialized'):
                for k,v in base.items():
                    if k.startswith('items['):state['drafts']['items'][k]=v;state['applied']['items'][k]=copy.deepcopy(v)
                template=values(parse((ROOT/'item-1.html').read_text()).one(id='workspace-form'))
                for row in f.find(cls='episode-row'):
                    i=int(row.attrs['id'].split('-')[-1]);item=copy.deepcopy(template)
                    item.update(title=[row.one(cls='row-title').text()],first_aired=[''],season=[''],episode=[''],numbering_kind=['none'],draft_group=[f'item-{i}'])
                    state['drafts'][f'item-{i}']=item;state['applied'][f'item-{i}']=copy.deepcopy(item)
                state['large_initialized']=True
            for k,v in base.items():
                if k.startswith('items['):state['drafts']['items'].setdefault(k,v);state['applied']['items'].setdefault(k,v)
        if state['long'] and path in ['index.html','numbering.html','long-list.html','recovery-list.html']:file=list_file(state)
        tree=parse(file.read_text());main=tree.one(tag='main')
        if re.fullmatch(r'item-\d+\.html',path):
            i=int(re.search(r'\d+',path)[0]);state['current_item']=i
            if i>36:
                f=main.one(id='workspace-form');f.attrs['data-draft']=f'item-{i}'
                for n in f.find(tag='input'):
                    if n.attrs.get('name')=='draft_group':n.attrs['value']=f'item-{i}'
            if state.get('large'):
                main.one(tag='h1').children=[val(state['applied'][f'item-{i}'],'title')]
                main.one(cls='section-heading').one(tag='p').children=[f'Rank {i} · Review recommended · Date and runtime unknown']
        if path=='replacement.html':
            i=state.get('current_item',6);item=state['applied'].get(f'item-{i}',{});title=val(item,'title',f'Item {i}')
            f=main.one(id='workspace-form');f.attrs['data-draft']=f'replacement-{i}'
            for n in f.find(tag='input'):
                if n.attrs.get('name')=='draft_group':n.attrs['value']=f'replacement-{i}'
                if n.attrs.get('name')=='replacement_query':n.attrs['value']=title
            current=val(state['applied'].get(f'replacement-{i}',{}),'replacement','none')
            f.one(tag='p').children=[f'Item {i} · '+title+' · Current replacement: '+('None' if current=='none' else 'Synthetic local version')+'. Original source is retained.']
            for button in f.find(tag='button'):
                if button.attrs.get('value')=='nav:item-6.html':button.attrs['value']=f'nav:item-{i}.html'
            for td in f.find(tag='td'):
                if td.text()=='The sound of suspense':td.children=[title]
        if path in ['index.html','long-list.html','large-list.html','recovery-list.html']:
            title=val(state['applied'].get('settings',{}),'metadata_title','The Screening Room')
            main.one(tag='h1').children=[title]
            status=main.one(cls='publication-summary')
            if status:
                for old in list(status.find(cls='publication-recovery')):old.remove()
                published=state.get('published_revision',29);pending=state['revision']!=published;mode=state.get('publication_demo')
                status.one(tag='strong').children=['Saved changes ready to publish' if pending else 'Published list is up to date']
                status.one(tag='span').children=[f"{len(ids(state))} included · "+('165 need review · ' if state.get('large') else '')+('Publish to update the published list.' if pending else 'No saved changes waiting for publication.')]
                next(n for n in status.walk() if 'data-publication-revision' in n.attrs).children=[f"Snapshot {state.get('published_snapshot',3)} · published revision {published} · applied revision {state['revision']}"]
                for form in status.find(tag='form'):
                    for button in form.find(tag='button'):
                        if pending and mode not in ['recovery','running']:button.attrs.pop('disabled',None)
                        else:button.attrs['disabled']=None
                if mode in ['recovery','running']:
                    recovery=mode=='recovery';heading='Publication needs recovery' if recovery else 'Publication retry in progress'
                    status.one(tag='strong').children=['Published list remains available']
                    status.one(tag='span').children=['The pending publication has not replaced it.']
                    alert=Node('aside',{'class':'notice publication-recovery','role':'alert' if recovery else 'status','aria-labelledby':'recovery-title'},[Node('h2',{'id':'recovery-title'},[heading]),Node('p',children=['A file was in use during publication. Retry is available.' if recovery else 'The synthetic retry is running. Another publication cannot start yet.'])])
                    alert.add(Node('button',{'class':'button primary','form':'workspace-form','name':'intent','value':'nav:recovery.html','formnovalidate':None},['Review recovery options' if recovery else 'View publication progress']))
                    # Keep the problem and next action outside optional diagnostics.
                    status.children.insert(1,alert);alert.parent=status
        if path=='recovery.html' and state.get('publication_demo'):
            mode=state['publication_demo'];recovery=mode=='recovery';running=mode=='running'
            main.one(tag='h1').children=['Publication needs recovery' if recovery else 'Publication retry in progress' if running else 'Publication completed']
            f=main.one(id='workspace-form');f.children=[]
            f.add(Node('input',{'type':'hidden','name':'draft_group','value':'recovery'}))
            f.add(Node('p',children=['Published snapshot 3 is still available. Pending snapshot 4 could not replace it because a file was in use.' if recovery else 'Published snapshot 3 remains available while pending snapshot 4 is retried.' if running else 'Published snapshot 4 is available in this synthetic scenario.']))
            f.add(Node('p',children=[f"Generation {state.get('publication_generation',1)} · "+('Retry available now' if recovery else 'Retry running; another retry is unavailable' if running else 'Complete')]))
            if recovery or running:
                f.add(Node('button',{'class':'button primary','name':'intent','value':'demo-retry-publication' if recovery else 'demo-complete-publication'},['Retry pending publication' if recovery else 'Simulate successful completion']))
            f.add(Node('button',{'class':'button secondary','name':'intent','value':'nav:index.html','formnovalidate':None},['Back to items']))
            f.add(Node('p',{'class':'hint'},['Synthetic recovery demonstration; no provider calls or filesystem operations. Restore is unavailable in this scenario.']))
        if state['long'] and path=='numbering-preview.html':
            rows=main.one(cls='episode-rows');existing={r.attrs['id'] for r in rows.find(cls='episode-row')}
            for row in parse((list_file(state)).read_text()).find(cls='episode-row'):
                if row.attrs['id'] in existing:continue
                i=int(row.attrs['id'].split('-')[-1])
                for n in list(row.find(cls='row-command-trigger')+row.find(cls='row-commands')):n.remove()
                row.one(cls='row-pick').replace(Node('span',{'class':'row-index'},[str(i)]))
                row.one(cls='row-numbering').replace(Node('div',{'class':'review-pair','data-number-review':str(i)}))
                rows.add(row)
        if state['long'] and main.one(cls='episode-rows') and path!='numbering-preview.html':
            rows=main.one(cls='episode-rows');existing={r.attrs['id'] for r in rows.find(cls='episode-row')}
            for row in parse((list_file(state)).read_text()).find(cls='episode-row'):
                if row.attrs['id'] not in existing:rows.add(copy.deepcopy(row))
            for select in main.find(tag='select'):
                if re.fullmatch(r'range\[\d+\]\.end_id',select.attrs.get('name','')):
                    start=int(re.search(r'\d+',select.attrs['name'])[0]);select.children=[]
                    for i in range(start,max(ids(state))+1):select.add(Node('option',{'value':str(i)},[f'#{i} · '+('Archive discussion '+str(i) if i>9 else 'Item '+str(i))]))
        if state.get('context') and path in ['index.html','long-list.html','large-list.html']:
            anchor=state['anchor'];row=main.one(id=f'episode-{anchor}')
            if row:
                choices=''.join(f'<option value="{i}">#{i} · Archive discussion {i}</option>' for i in range(anchor,max(ids(state))+1))
                surface=f'<div class="row-edit-surface"><h3>{"Select range" if state["context"]=="open-selection" else "Number from here"}</h3><label>Through item<select name="range[{anchor}].end_id">{choices}</select></label>'
                if state['context']=='open-numbering':
                    surface+=f'<div class="context-fields"><label>Season<input name="range[{anchor}].season" value="4" type="number"></label><label>First episode<input name="range[{anchor}].first_episode" value="1" type="number"></label><label>Minimum length<input name="range-{anchor}.minimum_length" placeholder="Unbounded"></label><label>Maximum length<input name="range-{anchor}.maximum_length" placeholder="Unbounded"></label></div><label class="check"><input type="checkbox" name="range[{anchor}].replace_existing" value="true">Replace existing numbers</label><label class="check"><input type="checkbox" name="range-{anchor}.include_unknown_length" value="true">Include unknown lengths</label>'
                    surface+=f'<details class="quiet-details"><summary>Numbering rules</summary><div><label>Maximum assignments<input name="range[{anchor}].count" type="number" min="1"></label><label class="check"><input name="range[{anchor}].detect_dates" type="checkbox" value="true">Detect seasons from air-date gaps</label><label>Minimum gap (days)<input name="range[{anchor}].gap_days" type="number" value="90" min="1"></label><p>Gap detection starts each detected season at episode 1. It cannot classify trailers or specials.</p></div></details>'
                surface+='<p>Lengths use minutes:seconds; blank means unbounded. Bounds include their endpoints.</p>' if state['context']=='open-numbering' else ''
                surface+=f'<button class="button primary" name="intent" value="{"select-range" if state["context"]=="open-selection" else "preview-range"}:{anchor}">{"Select range" if state["context"]=="open-selection" else "Preview numbering"}</button><button class="button text-action" name="intent" value="cancel-context" formnovalidate>Cancel</button></div>'
                row.add(fragment(surface))
        for f in main.find(tag='form'):
            g=f.attrs.get('data-draft','misc')
            if g not in state['drafts']:
                state['drafts'][g]=values(f);state['applied'][g]=copy.deepcopy(state['drafts'][g])
            if re.fullmatch(r'item-\d+',g):
                i=int(g[5:])
                for kind in ['drafts','applied']:
                    p=pair(state[kind]['items'],i)
                    for key,value in zip(['season','episode'],p):state[kind][g][key]=[value]
                    state[kind][g]['numbering_kind']=['numbered' if p!=('','') else 'none']
            overlay(f,state['drafts'].get(g,{}))
            if path=='recovery.html' and state.get('publication_demo'):
                f.add(Node('input',{'type':'hidden','name':'publication_generation','value':str(state.get('publication_generation',1))}))
            if re.fullmatch(r'item-\d+',g):
                for disclosure in f.find(tag='details'):
                    if disclosure.attrs.get('id') in ['item-metadata','item-chapters']:
                        fields=[n.attrs.get('name') for n in disclosure.walk() if n.tag in ['input','textarea']]
                        if any(val(state['drafts'][g],k)!=val(state['applied'][g],k) for k in fields):disclosure.attrs['open']=None
            checkbox_names=','.join(dict.fromkeys(n.attrs['name'] for n in f.find(tag='input') if n.attrs.get('type')=='checkbox' and n.attrs.get('name')))
            for n in f.find(tag='input'):
                if n.attrs.get('name')=='checkbox_names':n.attrs['value']=checkbox_names
            f.add(fragment('<input type="hidden" name="csrf" value="'+state['csrf']+'"><input type="hidden" name="origin" value="'+path+'">'))
        if re.fullmatch(r'item-\d+\.html',path):
            i=int(re.search(r'\d+',path)[0]);nav=main.one(cls='item-navigation');order=eligible_rows(state)
            if nav and i in order:
                nav.children=[];position=order.index(i)
                for offset,label in [(-1,'← Previous'),(1,'Next →')]:
                    attrs={'form':'workspace-form','class':'button secondary','name':'intent','formnovalidate':None}
                    if 0<=position+offset<len(order):attrs['value']=f'nav:item-{order[position+offset]}.html'
                    else:attrs['disabled']=None
                    nav.add(Node('button',attrs,[label]))
                nav.add(Node('button',{'form':'workspace-form','class':'button secondary','name':'intent','value':'nav:access-result.html'},['Test saved access']))
                nav.add(Node('button',{'form':'workspace-form','class':'button secondary','name':'intent','value':f'nav:index.html#episode-{i}','formnovalidate':None},['Back to items']))
        if state['error'] and not state.get('numbering_error_ids'):
            # Preserve reachable invalid controls; CSS cannot manage conditional validity.
            owner=main.one(id='workspace-form')
            if owner and owner.attrs.get('data-draft')==state.get('error_group'):
                for de in owner.find(tag='details'):de.attrs['open']=None
        saved=main.one(cls='saved-appearance')
        if saved and saved.one(tag='strong'):saved.one(tag='strong').children=[val(state['applied'].get('settings',{}),'metadata_title','The Screening Room')]
        for row in main.find(cls='episode-row'):
            i=int(row.attrs['id'].split('-')[-1]);item=state['applied'].get(f'item-{i}',{})
            title=row.one(cls='row-title')
            if title and val(item,'title'):title.children=[val(item,'title')]
        f=main.one(id='workspace-form')
        if f and f.attrs.get('data-draft')=='items':
            shown=page_ids(state) if path in ['index.html','long-list.html','large-list.html','recovery-list.html'] or state['long'] and path=='numbering.html' else ids(state)
            for row in list(f.find(cls='episode-row')):
                i=int(row.attrs['id'].split('-')[-1])
                if i not in shown:row.remove();continue
                sm=row.one(cls='row-numbering').one(tag='small')
                if sm:sm.children=['Saved: '+fmt(pair(state['applied']['items'],i))]
            f.add(fragment('<input type="hidden" name="visible_ids" value="'+','.join(map(str,shown))+'">'))
            chosen=state['drafts']['items'].get('selected_ids',[]);off=sum(x not in list(map(str,shown)) for x in chosen)
            group_actions=f.one(id='group-actions')
            if group_actions and (chosen or val(state['drafts']['items'],'bulk_scope','checked')!='checked'):
                group_actions.attrs['open']=None
            savebar=f.one(cls='editor-savebar')
            if savebar and 'undo' in state:
                recovery=Node('div',{'class':'numbering-history'},[Node('span',children=['Last applied numbering']),Node('button',{'class':'button secondary','name':'intent','value':'undo-numbers'},['Undo numbering'])])
                savebar.parent.children.insert(savebar.parent.children.index(savebar),recovery)
            if 'bulk_undo' in state and f.one(cls='bulk-apply-row'):
                f.one(cls='bulk-apply-row').add(Node('button',{'class':'button secondary','name':'intent','value':'undo-bulk'},['Undo last bulk change']))
            label=f.one(cls='selection-label')
            if label:label.children=[f'{len(chosen)} selected'+(f' · {off} on other pages' if off else '')]
            for sel in f.find(tag='select'):
                if sel.attrs.get('name')=='review_category':
                    for opt in sel.find(tag='option'):
                        original=opt.attrs.get('value','');label_text=original.split(' (')[0]
                        count=len(ids(state)) if label_text=='All items' else (sum(pair(state['drafts']['items'],i)==('','') for i in ids(state)) if label_text=='Unnumbered' else (165 if state.get('large') and label_text=='Needs review' else 2 if state['long'] and not state.get('large') and label_text=='Needs review' else (1 if state['long'] and not state.get('large') and label_text in ['Excluded','Unavailable','Conflicted'] else 0)))
                        opt.children=[f'{label_text} ({count})']
            if not shown:
                f.one(cls='episode-rows').add(fragment('<li class="notice">No matching items. Your other edits and selection are retained.</li>'))
            if f.one(cls='pager'):
                all_rows=eligible_rows(state);at_end=not shown or shown[-1]==all_rows[-1]
                pager=f.one(cls='pager')
                pager.one(cls='page-summary').children=[f"Page {state['page']+1} · {len(shown)} shown / {len(all_rows)} matching"]
                for button in pager.find(tag='button'):
                    intent=button.attrs.get('value')
                    if intent not in ['previous-page','next-page']:continue
                    button.attrs.pop('disabled',None)
                    if state['page']==0 and intent=='previous-page' or at_end and intent=='next-page':button.attrs['disabled']=None
        if state['notice']:
            attrs={'class':'notice','role':'alert' if state['error'] else 'status'}
            if state['error']:
                attrs.update({'id':'error-summary','tabindex':'-1'})
                if state.pop('focus_error',False):attrs['autofocus']=None
            n=Node('div',attrs,[Node('p',children=[state['notice']])])
            heading=main.one(cls='page-heading')
            main.children.insert(main.children.index(heading)+1 if heading and heading.parent is main else 1,n)
            if state.get('item_error_field') and f and f.attrs.get('data-draft','').startswith('item-'):
                field=f.one(id=state['item_error_field'])
                if field:
                    field.attrs['aria-invalid']='true'
                    field.attrs['aria-describedby']=field.attrs.get('aria-describedby','')+' error-summary'
                    n.add(Node('a',{'href':'#'+state['item_error_field']},['Review chapter times']))
            conflicts=state.get('numbering_error_ids',[])
            if conflicts and f and f.attrs.get('data-draft')=='items':
                links=n.add(Node('ul'))
                source=parse((list_file(state)).read_text())
                for i in conflicts:
                    source_row=source.one(id=f'episode-{i}')
                    title=val(state['applied'].get(f'item-{i}',{}),'title',source_row.one(cls='row-title').text())
                    label=f'Item {i}: {title}'
                    row=main.one(id=f'episode-{i}')
                    fields=[] if row is None else [x for x in row.find(tag='input') if x.attrs.get('name') in [f'items[{i}].season',f'items[{i}].episode']]
                    if fields:
                        error_id=f'numbering-{i}-error'
                        row.one(cls='row-numbering').add(Node('p',{'id':error_id,'class':'numbering-error'},['Check season and episode. ',Node('a',{'href':'#error-summary'},['Review error'])]))
                        for field in fields:
                            kind='season' if field.attrs['name'].endswith('.season') else 'episode'
                            field.attrs.update({'id':f'numbering-{i}-{kind}','aria-invalid':'true','aria-describedby':error_id})
                        link=Node('a',{'href':f'#numbering-{i}-episode'},[label])
                    else:
                        link=Node('button',{'class':'text-action','form':'workspace-form','name':'intent','value':f'focus-numbering:{i}','formnovalidate':None},['Show '+label])
                    links.add(Node('li',children=[link]))
        if path=='numbering-preview.html' and state.get('preview'):
            p=state['preview'];summary=next(n for n in main.walk() if 'data-preview-summary' in n.attrs)
            summary.children=[f"{len(p['candidate'])} in scope · {len(p['changes'])} assignments · {len(p['reasons'])} skipped · list revision {p['revision']}"]
            for row in main.find(cls='episode-row'):
                i=int(row.attrs['id'].split('-')[-1]);box=row.one(cls='review-pair');old=pair(state['applied']['items'],i)
                proposed=p['changes'].get(i);reason=p['reasons'].get(i,'Outside scope')
                box.children=[fragment('<span>'+fmt(old)+'</span><strong>'+('→ '+fmt(proposed) if proposed else 'Unchanged')+'</strong>'),Node('small',children=['Selected' if proposed else reason])]
            for b in main.find(tag='button'):
                if b.attrs.get('value')=='apply-preview' and not p['changes']:b.attrs['disabled']=None
        if path=='bulk-preview.html' and state.get('bulk'):
            out=next(n for n in main.walk() if 'data-bulk-result' in n.attrs);out.children=[];bulk=state['bulk']
            out.add(Node('p',children=['Exact item IDs: '+', '.join(map(str,bulk['ids']))+' · revision '+str(bulk['revision'])]))
            for key in ['kind','behavior','cookies','sponsor']:out.add(Node('p',children=[key.title()+': '+(bulk[key] or 'Keep current value')]))
        if path=='options.html':
            applied=state['applied'].get('playback',{})
            origins=state.get('option_origins',{'cookies':'Using add-on default','sponsor':'Using add-on default'})
            for n in main.walk():
                if n.attrs.get('data-saved')=='cookies':n.children=['Saved: '+(val(applied,'cookie_browser') if val(applied,'cookies_enabled')=='true' else 'No cookies')+' · '+origins['cookies']]
                if n.attrs.get('data-saved')=='sponsor':n.children=['Saved: '+('On' if val(applied,'sponsor_enabled')=='true' else 'Off')+' · '+origins['sponsor']]
                if 'data-cutoff' in n.attrs:
                    div=n.one(tag='div');div.children=[Node('p',children=['Download videos imported after '+state['cutoff']+'. Earlier videos keep streaming.' if state.get('cutoff') else 'New means imported after you first save this mode. Earlier videos keep streaming.'])]
                    if state.get('cutoff'):n.attrs['data-has-cutoff']=''
        for details in main.find(tag='details'):
            if 'data-disclosure-kind' not in details.attrs:
                details.attrs['data-disclosure-kind']='controls';summary=details.one(tag='summary')
                summary.children=[Node('span',{'class':'disclosure-label'},summary.children),Node('span',{'class':'disclosure-toggle','aria-hidden':'true'})]
        for control in main.walk():
            classes=control.attrs.get('class','').split()
            if control.tag in ['button','a'] and 'text-action' in classes:
                control.attrs['class']=' '.join(dict.fromkeys([c for c in classes if c!='text-action']+['button','secondary']))
        data=('<!doctype html>\n'+tree.html()).encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Cache-Control','no-store');self.send_header('Set-Cookie',f'ux_preview={sid}; HttpOnly; SameSite=Strict; Path=/');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        if urlsplit(self.path).path!='/_preview':self.send_error(404);return
        sid,state=self.session()
        try:
            length=int(self.headers.get('Content-Length','0'))
            if length>2_000_000:self.send_error(413);return
            raw=self.rfile.read(length);ctype=self.headers.get('Content-Type','')
            if ctype.startswith('multipart/form-data'):
                msg=BytesParser(policy=default).parsebytes(('Content-Type: '+ctype+'\r\nMIME-Version: 1.0\r\n\r\n').encode()+raw);data={}
                for part in msg.iter_parts():
                    name=part.get_param('name',header='content-disposition');filename=part.get_filename()
                    value=filename if filename is not None else part.get_content()
                    if name:data.setdefault(name,[]).append(str(value))
            else:data=parse_qs(raw.decode(),keep_blank_values=True,max_num_fields=3000)
            if val(data,'csrf')!=state['csrf']:self.send_error(403);return
            origin=val(data,'origin','index.html')
            if not re.fullmatch(r'[a-z0-9-]+\.html',origin):self.send_error(400);return
            destination=act(state,data,origin)
            if val(data,'draft_group')=='playback' and val(data,'intent')=='apply':state['playback_applied']=True
        except (ValueError,KeyError,IndexError) as error:
            notice(state,str(error),True);destination=val(data,'origin','index.html') if 'data' in locals() else 'index.html'
            state['focus_error']=True
            state['error_group']=val(data,'draft_group') if 'data' in locals() else ''
            if isinstance(error,NumberingError):state['numbering_error_ids']=error.item_ids
            if isinstance(error,ItemMetadataError):state['item_error_field']='chapter_input'
        if not re.fullmatch(r'[a-z0-9-]+\.html(?:#[a-zA-Z0-9_-]+)?',destination):self.send_error(400);return
        self.send_response(303);self.send_header('Location','/'+destination);self.send_header('Set-Cookie',f'ux_preview={sid}; HttpOnly; SameSite=Strict; Path=/');self.send_header('Content-Length','0');self.end_headers()

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--port',type=int,default=8412);args=parser.parse_args()
    print(f'Synthetic HTML/CSS preview: http://127.0.0.1:{args.port}/index.html',flush=True)
    HTTPServer(('127.0.0.1',args.port),Handler).serve_forever()

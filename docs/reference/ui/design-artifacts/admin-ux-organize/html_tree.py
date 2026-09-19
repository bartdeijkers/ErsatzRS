"""Small HTML tree helper for the synthetic, script-free preview server."""
from html import escape
from html.parser import HTMLParser

VOID = set('area base br col embed hr img input link meta param source track wbr'.split())

class Node:
    def __init__(self, tag='', attrs=None, children=None):
        self.tag, self.attrs, self.children, self.parent = tag, dict(attrs or []), children or [], None
        for child in self.children:
            if isinstance(child, Node): child.parent = self
    def add(self, child):
        if isinstance(child, Node): child.parent = self
        self.children.append(child)
        return child
    def walk(self):
        yield self
        for child in self.children:
            if isinstance(child, Node): yield from child.walk()
    def find(self, tag=None, cls=None, id=None):
        return [n for n in self.walk() if (tag is None or n.tag == tag) and (cls is None or cls in n.attrs.get('class','').split()) and (id is None or n.attrs.get('id') == id)]
    def one(self, **kw):
        return next(iter(self.find(**kw)), None)
    def remove(self):
        if self.parent and self in self.parent.children: self.parent.children.remove(self)
    def replace(self, new):
        p=self.parent
        if p:
            i=p.children.index(self);p.children[i]=new;new.parent=p
    def text(self):
        return ''.join(c.text() if isinstance(c, Node) else c for c in self.children)
    def html(self):
        body=''.join(c.html() if isinstance(c,Node) else escape(c, quote=False) for c in self.children)
        if not self.tag:return body
        attrs=''.join(' '+k if v is None else ' '+k+'="'+escape(str(v),quote=True)+'"' for k,v in self.attrs.items())
        return '<'+self.tag+attrs+'>'+('' if self.tag in VOID else body+'</'+self.tag+'>')

class Parser(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True);self.root=Node();self.stack=[self.root]
    def handle_starttag(self, tag, attrs):
        n=self.stack[-1].add(Node(tag,attrs))
        if tag not in VOID:self.stack.append(n)
    def handle_startendtag(self, tag, attrs):self.stack[-1].add(Node(tag,attrs))
    def handle_endtag(self, tag):
        for i in range(len(self.stack)-1,0,-1):
            if self.stack[i].tag==tag:self.stack=self.stack[:i];break
    def handle_data(self, data):self.stack[-1].add(data)

def parse(html):
    p=Parser();p.feed(html);return p.root

def fragment(html):
    return parse(html)

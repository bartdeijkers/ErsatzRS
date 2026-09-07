// Network-free provider for the M02 host acceptance only. No production add-on code.
export function subtitleProviderSource(invocationLog: string, records: unknown[]): string {
  return `
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { spawn, execFileSync } = require('child_process');
const command = process.argv[2];
const output = value => process.stdout.write(JSON.stringify(value) + '\\n');
const log = ${JSON.stringify(invocationLog)};
fs.appendFileSync(log, JSON.stringify({command}) + '\\n');
const records = ${JSON.stringify(records)};
if (command === 'check') output({status:'ready',code:'ready',message:'Synthetic subtitles ready'});
else if (command === 'discover' || command === 'list') {
  if (command === 'discover') {
    const request = JSON.parse(fs.readFileSync(0,'utf8'));
    const delivery = new URL(request.source_url).searchParams.get('delivery');
    if (!['stream','download_ahead'].includes(delivery)) throw new Error('Unexpected fixture list');
    records.find(record => record.record_type === 'list').provider_id += '-'+delivery;
    records.find(record => record.record_type === 'list').name += ' '+delivery;
  }
  for (const record of records) if (command === 'discover' || record.record_type !== 'page') output(record);
} else {
  const request = JSON.parse(fs.readFileSync(0,'utf8'));
  const echo = {provider_id:request.provider_id,context_fingerprint:request.context_fingerprint,tool_version:request.tool_version};
  const subtitles = prepared => {
    if (JSON.stringify(request.requested_languages ?? request.requested_subtitle_languages) !== '["en"]') throw new Error('Unexpected subtitle request');
    const body = prepared
      ? '1\\n00:00:08,000 --> 00:00:40,000\\nMANUAL BEFORE\\n\\n2\\n00:00:40,000 --> 00:01:20,000\\nMANUAL AFTER\\n'
      : '1\\n00:00:08,000 --> 00:00:40,000\\nMANUAL BEFORE\\n\\n2\\n00:00:40,000 --> 00:00:44,000\\nREMOVED CUE\\n\\n3\\n00:00:44,000 --> 00:01:24,000\\nMANUAL AFTER\\n';
    fs.writeFileSync(path.join(request.staging_directory,'manual.en.srt'),body);
    return [{language:'en',format:'srt',provenance:'manual',timeline:prepared?'prepared':'source',
      relative_path:'manual.en.srt',byte_length:Buffer.byteLength(body),sha256:crypto.createHash('sha256').update(body).digest('hex')}];
  };
  const mediaArgs = ranges => {
    if (!ranges.length || ranges.length > 2 || ranges.some(r => r.start < 0 || r.end > 90000 || r.start >= r.end)) throw new Error('Invalid fixture ranges');
    const n = ranges.length;
    const labels = ranges.map((_,i) => '[v'+i+']').join('');
    const audio = ranges.map((_,i) => '[a'+i+']').join('');
    // Source-clock phase colors make retained-cut visibility inspectable without
    // OCR or private media: green exists only inside the excluded source span.
    const filters = ["[0:v]drawbox=color=green:t=fill:enable='gte(t,40)*lt(t,44)',drawbox=color=blue:t=fill:enable='gte(t,44)',split="+n+labels, '[1:a]asplit='+n+audio];
    ranges.forEach((r,i) => {
      filters.push('[v'+i+']trim=start='+r.start/1000+':end='+r.end/1000+',setpts=PTS-STARTPTS[vo'+i+']');
      filters.push('[a'+i+']atrim=start='+r.start/1000+':end='+r.end/1000+',asetpts=PTS-STARTPTS[ao'+i+']');
    });
    filters.push(ranges.map((_,i) => '[vo'+i+'][ao'+i+']').join('')+'concat=n='+n+':v=1:a=1[v][a]');
    return ['-hide_banner','-nostdin','-loglevel','error','-y','-filter_complex_threads','1',
      '-f','lavfi','-i','color=c=red:size=320x180:rate=25:duration=90',
      '-f','lavfi','-i','sine=frequency=1000:sample_rate=48000:duration=90',
      '-filter_complex',filters.join(';'),'-map','[v]','-map','[a]','-threads','1'];
  };
  if (command === 'runtime-info') output({tool_version:'fixture-1'});
  else if (command === 'interval-metadata') output({...echo,outcome:{kind:'ready',source_duration_milliseconds:90000,excluded_intervals_milliseconds:[{start:40000,end:44000}]}});
  else if (command === 'subtitles') output({...echo,subtitles:subtitles(false)});
  else if (command === 'play-intervals') {
    fs.appendFileSync(log,JSON.stringify({command:'ranges',ranges:request.retained_intervals_milliseconds})+'\\n');
    const child = spawn(process.env.FFMPEG_BIN,[...mediaArgs(request.retained_intervals_milliseconds),
      '-c:v','rawvideo','-pix_fmt','yuv420p','-c:a','pcm_f32le','-f','nut','pipe:1'],{stdio:['ignore','inherit','inherit']});
    child.on('error',() => process.exit(1));
    child.on('exit',code => process.exit(code ?? 1));
  } else if (command === 'prepare') {
    const media = path.join(request.staging_directory,'prepared.mkv');
    execFileSync(process.env.FFMPEG_BIN,[...mediaArgs([{start:0,end:40000},{start:44000,end:90000}]),
      '-c:v','mpeg2video','-g','12','-bf','0','-c:a','mp2',media],{timeout:60000,stdio:['ignore','ignore','pipe']});
    const bytes = fs.readFileSync(media);
    output({...echo,selected_format:'synthetic-mpeg2',relative_path:'prepared.mkv',byte_length:bytes.length,
      sha256:crypto.createHash('sha256').update(bytes).digest('hex'),duration_milliseconds:86000,subtitles:subtitles(true)});
  } else process.exit(77);
}
`;
}

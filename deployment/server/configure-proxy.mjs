// Run inside this server's Nginx Proxy Manager container, using its own model.
// The short-lived management token remains in memory and is never printed.
import Token from '/app/models/token.js';
const domain = process.env.PCMIRROR_DOMAIN || 'pcmirror.oel.best';
const signed = await Token().create({iss:'api',attrs:{id:Number(process.env.NPM_ADMIN_ID || 1)},scope:['user'],expiresIn:'5m'});
async function request(method, path, body) {
    const response = await fetch('http://127.0.0.1:3000'+path, {
        method, headers:{Authorization:'Bearer '+signed.token,'Content-Type':'application/json'},
        body:body === undefined ? undefined : JSON.stringify(body), signal:AbortSignal.timeout(150000)
    });
    const result = await response.json();
    if (!response.ok) throw new Error('NPM '+method+' '+path+': '+response.status+' '+JSON.stringify(result));
    return result;
}
try {
    const hosts = await request('GET','/nginx/proxy-hosts');
    let host = hosts.find(h=>h.domain_names.includes(domain));
    if (host && host.forward_host !== 'pcmirror-updates') throw new Error('Domain already points to a different service');
    const data = {domain_names:[domain],forward_scheme:'http',forward_host:'pcmirror-updates',forward_port:8080,
        certificate_id:0,ssl_forced:false,http2_support:false,hsts_enabled:false,hsts_subdomains:false,
        trust_forwarded_proto:false,access_list_id:0,caching_enabled:false,block_exploits:true,
        allow_websocket_upgrade:false,advanced_config:'',meta:{},locations:[],enabled:true};
    if (!host) host = await request('POST','/nginx/proxy-hosts',data);
    console.log('Update proxy host:',host.id);
    const certificates = await request('GET','/nginx/certificates');
    let certificate = certificates.find(c=>c.provider==='letsencrypt' && c.domain_names.includes(domain));
    if (!certificate) certificate = await request('POST','/nginx/certificates',{
        provider:'letsencrypt',domain_names:[domain],meta:{dns_challenge:false}
    });
    host = await request('PUT','/nginx/proxy-hosts/'+host.id,{...data,certificate_id:certificate.id,ssl_forced:true,http2_support:true});
    console.log('Managed HTTPS enabled; certificate:',certificate.id);
    console.log('Public update URL: https://'+domain);
    process.exit(0);
} catch (error) {
    console.error(error.message);process.exit(1);
}

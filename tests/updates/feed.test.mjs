import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, createHash, verify } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { signedFeed, keyId, targets } from '../../tools/update-feed.mjs';

function fixture() {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fleab-feed-'));
    const pair = generateKeyPairSync('ed25519');
    const keys = { [keyId]: Buffer.from(pair.publicKey.export({format:'jwk'}).x,'base64url').toString('base64') };
    const pem = pair.privateKey.export({format:'pem',type:'pkcs8'});
    for(const target of targets) {
        const filename = `Fleab-1.2.3-update-${target}.tar.gz`;
        const bytes = Buffer.from(`test package ${target}`);
        fs.writeFileSync(path.join(dir, filename), bytes);
        fs.writeFileSync(path.join(dir,filename+'.json'),JSON.stringify({ filename,size:bytes.length,
            sha256:createHash('sha256').update(bytes).digest('hex'), executable_sha256:'a'.repeat(64), unpacked_size:100 }));
    }
    return {dir,pair,keys,pem};
}
function cleanup(f) {
    assert.equal(path.dirname(path.resolve(f.dir)),path.resolve(os.tmpdir()));
    assert(path.basename(f.dir).startsWith('fleab-feed-'));
    fs.rmSync(f.dir,{recursive:true,force:true});
}
test('six-platform feed authenticates the exact payload with Ed25519',()=>{
    const f=fixture(); try {
        const envelope=signedFeed(f.dir,'1.2.3',f.pem,f.keys);
        const payload=Buffer.from(envelope.payload,'base64');
        assert(verify(null,payload,f.pair.publicKey,Buffer.from(envelope.signature,'base64')));
        assert.equal(JSON.parse(payload).version,'1.2.3');
        assert.deepEqual(Object.keys(JSON.parse(payload).assets),targets);
        payload[0]^=1;
        assert(!verify(null,payload,f.pair.publicKey,Buffer.from(envelope.signature,'base64')));
    } finally {cleanup(f);}
});
test('missing, modified and mismatched target packages cannot be published',()=>{
    const f=fixture(); try {
        const file=path.join(f.dir,'Fleab-1.2.3-update-windows-x64.tar.gz');
        fs.appendFileSync(file,'corruption');
        assert.throws(()=>signedFeed(f.dir,'1.2.3',f.pem,f.keys),/metadata/);
        fs.unlinkSync(file);
        assert.throws(()=>signedFeed(f.dir,'1.2.3',f.pem,f.keys));
    } finally {cleanup(f);}
});
test('unknown signing keys and unstable versions are rejected',()=>{
    const f=fixture(); try {
        assert.throws(()=>signedFeed(f.dir,'1.2.3',f.pem,{[keyId]:'incorrect'}),/match/);
        for(const version of ['v1.2.3','1.2.3-beta.1','1.2.3+build','01.2.3','../../evil'])
            assert.throws(()=>signedFeed(f.dir,version,f.pem,f.keys),/stable/);
    } finally {cleanup(f);}
});

// Read CAB containers from the signed Burn archive; never execute the installer.
const fs = require('node:fs'), path = require('node:path');
const input=fs.readFileSync(process.argv[2]);let index=0;
for(let offset=0;(offset=input.indexOf(Buffer.from('MSCF'),offset))>=0;offset+=4) {
  if(offset+36>input.length) break;
  const size=input.readUInt32LE(offset+8);
  if(input.readUInt32LE(offset+4)===0 && size>36 && offset+size<=input.length)
    fs.writeFileSync(path.join(process.argv[3],`container-${index++}.cab`),input.subarray(offset,offset+size));
}
if(index<2)throw new Error('VC archive does not contain the expected CAB containers');

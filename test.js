const http = require('http')
const port = 3000
const host = 'localhost'
 
const server = http.createServer(function (req, res) {
    if (req.url.includes('/a/a/a/')){
res.end("lmaoooo")
    }
    res.writeHead(302, {
        'Location': "http://localhost:3000"+req.url+'/a',
        'kekw':'72727'
        //add other headers here...
      });
      res.end();
})
 
server.listen(port, host, function () {
    console.log('Web server is running on port 3000')
})
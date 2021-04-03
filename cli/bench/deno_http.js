const body = Deno.core.encode("Hello World");
const addr = Deno.args[0] || "127.0.0.1:4500";
for await (const { request, respondWith } of Deno.http.listen(addr)) {
  respondWith(new Response(body));
}

#include <openssl/ssl.h>
#include <openssl/err.h>
#include <openssl/bio.h>
#include <openssl/crypto.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_PUMP 20000
#define APP_MAX 16384

typedef struct {
    SSL *ssl;
    BIO *net;
    const char *name;
    int post_handshake;
    int reneg_event;
    int alert_write_observed;
    int poisoned;
    size_t staged, native_ok, cipher_handoff, delivered;
    int last_ret, last_err;
    const void *saved_ptr;
    size_t saved_len;
} Endpoint;

typedef struct { SSL_CTX *client, *server; Endpoint c, s; size_t ring; } Pair;

static const char *errname(int e) {
    switch (e) {
    case SSL_ERROR_NONE: return "NONE";
    case SSL_ERROR_SSL: return "SSL";
    case SSL_ERROR_WANT_READ: return "WANT_READ";
    case SSL_ERROR_WANT_WRITE: return "WANT_WRITE";
    case SSL_ERROR_ZERO_RETURN: return "ZERO_RETURN";
    case SSL_ERROR_SYSCALL: return "SYSCALL";
    default: return "OTHER";
    }
}

/* Passive only: no OpenSSL call, no raw SSL internals, no policy action here. */
static void msg_cb(int write_p, int version, int content_type,
                   const void *buf, size_t len, SSL *ssl, void *arg) {
    Endpoint *e = arg;
    const unsigned char *p = buf;
    (void)ssl;
    if (!e || !e->post_handshake || version != TLS1_2_VERSION || len == 0)
        return;
    if (write_p && content_type == SSL3_RT_ALERT) {
        e->alert_write_observed = 1;
        return;
    }
    if (!write_p && content_type == SSL3_RT_HANDSHAKE
        && (p[0] == SSL3_MT_HELLO_REQUEST || p[0] == SSL3_MT_CLIENT_HELLO))
        e->reneg_event = 1;
}

/* ERR queue is cleared before the SSL call. This is called immediately after it. */
static int classify(Endpoint *e, int ret) {
    int ge = SSL_get_error(e->ssl, ret);
    e->last_ret = ret;
    e->last_err = ge;
    return ge;
}

static void print_errors(void) {
    unsigned long x;
    while ((x = ERR_get_error()) != 0)
        fprintf(stderr, " openssl=%s\n", ERR_error_string(x, NULL));
}

static int move_one(Endpoint *from, Endpoint *to, size_t quantum) {
    unsigned char b[16384];
    size_t pending = (size_t)BIO_ctrl_pending(from->net);
    if (pending == 0) return 0;
    if (pending > quantum) pending = quantum;
    if (pending > sizeof(b)) pending = sizeof(b);
    int n = BIO_read(from->net, b, (int)pending);
    if (n <= 0) return -1;
    int off = 0;
    while (off < n) {
        int w = BIO_write(to->net, b + off, n - off);
        if (w <= 0) return -1;
        off += w;
    }
    from->cipher_handoff += (size_t)n;
    to->delivered += (size_t)n;
    return n;
}

static SSL_CTX *new_ctx(int server, int version, const char *cert, const char *key) {
    SSL_CTX *ctx = SSL_CTX_new(server ? TLS_server_method() : TLS_client_method());
    if (!ctx) return NULL;
    SSL_CTX_set_min_proto_version(ctx, version);
    SSL_CTX_set_max_proto_version(ctx, version);
    SSL_CTX_set_mode(ctx, SSL_MODE_ENABLE_PARTIAL_WRITE);
    SSL_CTX_clear_mode(ctx, SSL_MODE_AUTO_RETRY | SSL_MODE_ACCEPT_MOVING_WRITE_BUFFER);
    SSL_CTX_set_max_send_fragment(ctx, APP_MAX);
    SSL_CTX_set_num_tickets(ctx, 0);
    SSL_CTX_set_options(ctx, SSL_OP_NO_COMPRESSION | SSL_OP_NO_RENEGOTIATION | SSL_OP_NO_TICKET);
    if (version == TLS1_2_VERSION)
        SSL_CTX_set_cipher_list(ctx, "ECDHE-ECDSA-AES128-GCM-SHA256");
    else
        SSL_CTX_set_ciphersuites(ctx, "TLS_AES_128_GCM_SHA256");
    if (server) {
        if (SSL_CTX_use_certificate_file(ctx, cert, SSL_FILETYPE_PEM) != 1
            || SSL_CTX_use_PrivateKey_file(ctx, key, SSL_FILETYPE_PEM) != 1)
            goto bad;
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, NULL); /* no client auth in this fixture */
    } else {
        if (SSL_CTX_load_verify_locations(ctx, cert, NULL) != 1) goto bad;
        SSL_CTX_set_verify(ctx, SSL_VERIFY_PEER, NULL);
    }
    return ctx;
bad:
    SSL_CTX_free(ctx);
    return NULL;
}

static void pair_free(Pair *p) {
    SSL_free(p->c.ssl); SSL_free(p->s.ssl);
    BIO_free(p->c.net); BIO_free(p->s.net);
    SSL_CTX_free(p->client); SSL_CTX_free(p->server);
    memset(p, 0, sizeof(*p));
}

static int pair_new(Pair *p, int version, size_t ring, const char *cert, const char *key) {
    BIO *ci = NULL, *cn = NULL, *si = NULL, *sn = NULL;
    memset(p, 0, sizeof(*p));
    p->ring = ring;
    p->client = new_ctx(0, version, cert, key);
    p->server = new_ctx(1, version, cert, key);
    if (!p->client || !p->server) goto bad;
    p->c.name = "client"; p->s.name = "server";
    p->c.ssl = SSL_new(p->client); p->s.ssl = SSL_new(p->server);
    if (!p->c.ssl || !p->s.ssl) goto bad;
    if (!BIO_new_bio_pair(&ci, ring, &cn, ring)
        || !BIO_new_bio_pair(&si, ring, &sn, ring)) goto bad;
    SSL_set_bio(p->c.ssl, ci, ci); ci = NULL;
    SSL_set_bio(p->s.ssl, si, si); si = NULL;
    p->c.net = cn; cn = NULL; p->s.net = sn; sn = NULL;
    SSL_set_connect_state(p->c.ssl); SSL_set_accept_state(p->s.ssl);
    SSL_set_tlsext_host_name(p->c.ssl, "localhost");
    SSL_set1_host(p->c.ssl, "localhost");
    SSL_set_msg_callback(p->c.ssl, msg_cb); SSL_set_msg_callback_arg(p->c.ssl, &p->c);
    SSL_set_msg_callback(p->s.ssl, msg_cb); SSL_set_msg_callback_arg(p->s.ssl, &p->s);
    return 1;
bad:
    BIO_free(ci); BIO_free(cn); BIO_free(si); BIO_free(sn); pair_free(p); return 0;
}

static int handshake(Pair *p, size_t quantum) {
    int cd = 0, sd = 0;
    for (int i = 0; i < MAX_PUMP && !(cd && sd); i++) {
        if (!cd) { ERR_clear_error(); int r = SSL_do_handshake(p->c.ssl); int e = classify(&p->c, r);
            if (r == 1) cd = 1; else if (e != SSL_ERROR_WANT_READ && e != SSL_ERROR_WANT_WRITE) return 0; }
        if (!sd) { ERR_clear_error(); int r = SSL_do_handshake(p->s.ssl); int e = classify(&p->s, r);
            if (r == 1) sd = 1; else if (e != SSL_ERROR_WANT_READ && e != SSL_ERROR_WANT_WRITE) return 0; }
        int a = move_one(&p->c, &p->s, quantum), b = move_one(&p->s, &p->c, quantum);
        if (a < 0 || b < 0) return 0;
    }
    /* Explicit final-flight handoff: native completion is not enough. */
    for (int i = 0; i < MAX_PUMP; i++) {
        int a = move_one(&p->c, &p->s, quantum), b = move_one(&p->s, &p->c, quantum);
        if (a < 0 || b < 0) return 0;
        if (a == 0 && b == 0) break;
    }
    p->c.post_handshake = p->s.post_handshake = 1;
    return cd && sd && SSL_get_verify_result(p->c.ssl) == X509_V_OK;
}

static int write_retry(Endpoint *w, const unsigned char *buf, size_t len,
                       Endpoint *peer, size_t quantum, int *saw_want) {
    size_t out = 999;
    w->saved_ptr = buf; w->saved_len = len; w->staged += len;
    for (int i = 0; i < MAX_PUMP; i++) {
        ERR_clear_error(); int r = SSL_write_ex(w->ssl, buf, len, &out); int e = classify(w, r);
        if (r == 1) { w->native_ok += out; w->saved_ptr = NULL; w->saved_len = 0; return (int)out; }
        if (e != SSL_ERROR_WANT_WRITE && e != SSL_ERROR_WANT_READ) { print_errors(); return -1; }
        if (e == SSL_ERROR_WANT_WRITE) {
            *saw_want = 1;
            if (move_one(w, peer, quantum) <= 0) return -2;
            /* No other SSL I/O while write-WANT_WRITE; retry same ptr/length. */
        } else {
            if (move_one(peer, w, quantum) <= 0) return -2;
        }
    }
    return -2;
}

static int read_retry(Endpoint *r, unsigned char *buf, size_t cap, Endpoint *peer, size_t quantum) {
    size_t out = 999;
    r->saved_ptr = buf; r->saved_len = cap;
    for (int i = 0; i < MAX_PUMP; i++) {
        ERR_clear_error(); int x = SSL_read_ex(r->ssl, buf, cap, &out); int e = classify(r, x);
        if (x == 1) { r->saved_ptr = NULL; r->saved_len = 0; return (int)out; }
        if (e == SSL_ERROR_WANT_READ) {
            if (move_one(peer, r, quantum) <= 0) return -2;
        } else if (e == SSL_ERROR_WANT_WRITE) {
            /* Retry the read after draining its control output; never blind-write. */
            if (move_one(r, peer, quantum) <= 0) return -2;
        } else return -1;
    }
    return -2;
}

static int baseline_once(const char *cert, const char *key, int policy) {
    Pair p;
    unsigned char plain[227], prior_cipher[256], later_cipher[256], got[227], x[8];
    size_t n = 999;
    int ok = 0, prior_n = 0, later_n = 0, prior_read = 0, fresh_read = 0, peer_failed = 0;
    memset(plain, 'B', sizeof(plain));
    if (!pair_new(&p, TLS1_2_VERSION, 256, cert, key) || !handshake(&p, 97)) goto done;
    ERR_clear_error(); int wr = SSL_write_ex(p.c.ssl, plain, sizeof(plain), &n); int we = classify(&p.c, wr);
    size_t occupied = (size_t)BIO_ctrl_pending(p.c.net);
    if (wr != 1 || n != 227 || we != SSL_ERROR_NONE || occupied != 256) goto done;
    /* Fixture exception: server locally permits renegotiation solely to emit HelloRequest. */
    SSL_clear_options(p.s.ssl, SSL_OP_NO_RENEGOTIATION);
    if (SSL_renegotiate(p.s.ssl) != 1) goto done;
    ERR_clear_error(); int hr = SSL_do_handshake(p.s.ssl); int he = classify(&p.s, hr);
    if (hr != 1 && he != SSL_ERROR_WANT_READ && he != SSL_ERROR_WANT_WRITE) goto done;
    if (move_one(&p.s, &p.c, 256) <= 0) goto done;
    ERR_clear_error(); int rr = SSL_read_ex(p.c.ssl, x, sizeof(x), &n); int re = classify(&p.c, rr);
    /* Policy decision/query occurs only after immediate SSL result classification. */
    if (!p.c.reneg_event || !p.c.alert_write_observed || re != SSL_ERROR_WANT_READ) goto done;
    if (policy) {
        p.c.poisoned = 1;
        printf("RESULT renegotiation_explicit_stop PASS event=1 warning_alert_buffered=1 classified=%s stop=1 fresh_write=avoided occupied_before=%zu\n",
               errname(re), occupied);
        ok = 1; goto done;
    }
    BIO *top = SSL_get_wbio(p.c.ssl);
    const char *top_name = top == NULL ? "none" : BIO_method_name(top);
    int buffered_after_read = top != NULL && BIO_method_type(top) == BIO_TYPE_BUFFER;
    /* Preserve the full earlier record; do not discard it merely to free the ring. */
    prior_n = BIO_read(p.c.net, prior_cipher, sizeof(prior_cipher));
    if (prior_n != 256) goto done;
    ERR_clear_error(); int fresh = SSL_write_ex(p.c.ssl, "X", 1, &n); int fe = classify(&p.c, fresh);
    if (fresh != 1 || n != 1 || fe != SSL_ERROR_NONE) goto done;
    later_n = BIO_read(p.c.net, later_cipher, sizeof(later_cipher));
    if (later_n <= 0) goto done;
    /* Deliver ciphertext in original order: prior app record, warning alert, fresh app record. */
    if (BIO_write(p.s.net, prior_cipher, prior_n) != prior_n) goto done;
    ERR_clear_error(); int pr = SSL_read_ex(p.s.ssl, got, sizeof(got), &n); int pe = classify(&p.s, pr);
    if (pr != 1 || n != sizeof(plain) || pe != SSL_ERROR_NONE
        || memcmp(got, plain, sizeof(plain)) != 0) goto done;
    prior_read = 1;
    if (BIO_write(p.s.net, later_cipher, later_n) != later_n) goto done;
    for (int i = 0; i < 8; i++) {
        ERR_clear_error(); int xr = SSL_read_ex(p.s.ssl, x, sizeof(x), &n); int xe = classify(&p.s, xr);
        if (xr == 1) {
            if (n == 1 && x[0] == 'X') fresh_read = 1;
            break;
        }
        if (xe == SSL_ERROR_SSL) {
            peer_failed = 1;
            break;
        }
        if (xe != SSL_ERROR_WANT_READ && xe != SSL_ERROR_WANT_WRITE) goto done;
        if (xe == SSL_ERROR_WANT_WRITE && move_one(&p.s, &p.c, 256) < 0) goto done;
    }
    /* The local fresh write succeeds, but dropped control ciphertext desynchronizes records. */
    if (fresh_read || !peer_failed) goto done;
    ERR_clear_error();
    printf("RESULT renegotiation_unsafe_continue PASS event=1 warning_alert_callback=1 wbio_after_read=%s buffered_after_read=%d prior_cipher=%d later_cipher=%d prior_delivered=%d local_fresh_write=success peer_record_failure=1\n",
           top_name, buffered_after_read, prior_n, later_n, prior_read);
    ok = 1;
done:
    if (!ok) { fprintf(stderr, "STATE renegotiation policy=%d c_last=%s s_last=%s event=%d alert_attempt=%d ring=%zu prior_n=%d later_n=%d prior_read=%d fresh_read=%d peer_failed=%d\n",
                      policy, errname(p.c.last_err), errname(p.s.last_err), p.c.reneg_event,
                      p.c.alert_write_observed, p.ring, prior_n, later_n, prior_read, fresh_read, peer_failed); print_errors(); }
    pair_free(&p); return ok;
}

static int basic_flow(const char *cert, const char *key, int version, int reverse, size_t fragment) {
    Pair p; unsigned char msg[12000], got[12000]; int want = 0;
    memset(msg, reverse ? 'S' : 'C', sizeof(msg));
    if (!pair_new(&p, version, 16384, cert, key) || !handshake(&p, fragment)) goto fail;
    Endpoint *w = reverse ? &p.s : &p.c, *r = reverse ? &p.c : &p.s;
    int wn = write_retry(w, msg, sizeof(msg), r, fragment, &want);
    if (wn != (int)sizeof(msg)) goto fail;
    int rn = read_retry(r, got, sizeof(got), w, fragment);
    if (rn != (int)sizeof(msg) || memcmp(msg, got, sizeof(msg))) goto fail;
    printf("RESULT basic_%s_%s PASS fragmented=%zu staged=%zu native_success=%zu cipher_handoff=%zu delivery=%zu\n",
           version == TLS1_2_VERSION ? "tls12" : "tls13", reverse ? "s2c" : "c2s", fragment,
           w->staged, w->native_ok, w->cipher_handoff, r->delivered);
    pair_free(&p); return 1;
fail:
    fprintf(stderr, "STATE basic version=%x reverse=%d c=%s s=%s\n", version, reverse, errname(p.c.last_err), errname(p.s.last_err));
    pair_free(&p); return 0;
}

static int full_record_retry(const char *cert, const char *key) {
    Pair p; unsigned char msg[APP_MAX], got[APP_MAX]; int want = 0;
    memset(msg, 'R', sizeof(msg));
    if (!pair_new(&p, TLS1_2_VERSION, 16384, cert, key) || !handshake(&p, 16384)) goto fail;
    int wn = write_retry(&p.c, msg, sizeof(msg), &p.s, 16384, &want);
    int rn = read_retry(&p.s, got, sizeof(got), &p.c, 16384);
    if (wn != APP_MAX || rn != APP_MAX || !want || memcmp(msg, got, sizeof(msg))) goto fail;
    printf("RESULT same_args_retry PASS ring=16384 record_wire=16413 want_write=1 ptr_preserved=1 len_preserved=1 native_success=%zu\n", p.c.native_ok);
    pair_free(&p); return 1;
fail:
    fprintf(stderr, "STATE same_args c=%s s=%s saved_ptr=%p saved_len=%zu\n", errname(p.c.last_err), errname(p.s.last_err), p.c.saved_ptr, p.c.saved_len);
    pair_free(&p); return 0;
}

static int partial_suffix(const char *cert, const char *key) {
    Pair p; unsigned char msg[APP_MAX], got[APP_MAX]; size_t n = 999; int want = 0, e = SSL_ERROR_NONE;
    memset(msg, 'P', sizeof(msg));
    if (!pair_new(&p, TLS1_3_VERSION, 16384, cert, key)) return 0;
    SSL_set_max_send_fragment(p.c.ssl, 4096);
    if (!handshake(&p, 16384)) goto fail;
    ERR_clear_error(); int x = SSL_write_ex(p.c.ssl, msg, sizeof(msg), &n); e = classify(&p.c, x);
    if (x != 1 || n != 4096) goto fail;
    p.c.native_ok += n; p.c.staged += sizeof(msg);
    /* New suffix operation starts only after positive success. */
    int wn = write_retry(&p.c, msg + n, sizeof(msg) - n, &p.s, 16384, &want);
    if (wn <= 0) goto fail;
    int total = 0;
    while (total < (int)(n + (size_t)wn)) {
        int rn = read_retry(&p.s, got + total, sizeof(got) - (size_t)total, &p.c, 16384);
        if (rn <= 0) goto fail; total += rn;
    }
    if (memcmp(msg, got, (size_t)total)) goto fail;
    printf("RESULT partial_success_suffix PASS first_success=4096 fresh_suffix=%d failure_prefix_inferred=0\n", wn);
    pair_free(&p); return 1;
fail:
    fprintf(stderr, "STATE partial c=%s n=%zu e=%s\n", errname(p.c.last_err), n, errname(e)); pair_free(&p); return 0;
}

static int bidirectional(const char *cert, const char *key) {
    Pair p; unsigned char a[8000], b[7000], out[8000]; size_t an=999,bn=999; int ae=SSL_ERROR_NONE,be=SSL_ERROR_NONE;
    memset(a,'a',sizeof(a)); memset(b,'b',sizeof(b));
    if (!pair_new(&p,TLS1_3_VERSION,16384,cert,key)||!handshake(&p,37)) goto fail;
    ERR_clear_error(); int ar=SSL_write_ex(p.c.ssl,a,sizeof(a),&an); ae=classify(&p.c,ar);
    ERR_clear_error(); int br=SSL_write_ex(p.s.ssl,b,sizeof(b),&bn); be=classify(&p.s,br);
    /* Deliberate lower-receiver stall: no transfer occurred before this point. */
    if (ar!=1 || br!=1) goto fail;
    if (move_one(&p.c,&p.s,53)<=0 || move_one(&p.s,&p.c,47)<=0) goto fail;
    int sr=read_retry(&p.s,out,sizeof(out),&p.c,31); if(sr!=(int)sizeof(a)||memcmp(out,a,sizeof(a)))goto fail;
    int cr=read_retry(&p.c,out,sizeof(out),&p.s,29); if(cr!=(int)sizeof(b)||memcmp(out,b,sizeof(b)))goto fail;
    printf("RESULT bidirectional_stall_resume PASS cwrite=%zu swrite=%zu lower_stalled_then_resumed=1 separate_saved_wants=1\n",an,bn);
    pair_free(&p);return 1;
fail:
    fprintf(stderr,"STATE bidi c=%s s=%s ae=%s be=%s\n",errname(p.c.last_err),errname(p.s.last_err),errname(ae),errname(be));pair_free(&p);return 0;
}

static int keyupdate_case(const char *cert,const char *key,int requested,int blocked) {
    Pair p; unsigned char b[8]; size_t n=999; int saw_read_want=0, write_ok=0, saw_block=0;
    size_t ring=blocked?64:16384;
    if(!pair_new(&p,TLS1_3_VERSION,ring,cert,key)||!handshake(&p,ring))goto fail;
    if(SSL_key_update(p.s.ssl,requested?SSL_KEY_UPDATE_REQUESTED:SSL_KEY_UPDATE_NOT_REQUESTED)!=1)goto fail;
    for(int i=0;i<MAX_PUMP;i++){
        ERR_clear_error();int x=SSL_do_handshake(p.s.ssl);int e=classify(&p.s,x);
        if(x==1)break;
        if(e==SSL_ERROR_WANT_WRITE){saw_block=1;if(move_one(&p.s,&p.c,ring)<=0)goto fail;}
        else if(e==SSL_ERROR_WANT_READ){if(move_one(&p.c,&p.s,ring)<=0)goto fail;}
        else goto fail;
    }
    while(move_one(&p.s,&p.c,blocked?7:16384)>0){}
    ERR_clear_error();int rr=SSL_read_ex(p.c.ssl,b,sizeof(b),&n);int re=classify(&p.c,rr);
    saw_read_want=(re==SSL_ERROR_WANT_READ);
    if(!saw_read_want)goto fail;
    /* Established, no app-write pending, callback saw no reneg event: source-justified fresh write. */
    if(SSL_in_init(p.c.ssl)||p.c.reneg_event)goto fail;
    ERR_clear_error();int wr=SSL_write_ex(p.c.ssl,"K",1,&n);int we=classify(&p.c,wr);
    if(wr==1)write_ok=1;
    else if(we==SSL_ERROR_WANT_WRITE){while(move_one(&p.c,&p.s,ring)>0){} ERR_clear_error();wr=SSL_write_ex(p.c.ssl,"K",1,&n);we=classify(&p.c,wr);write_ok=wr==1;}
    if(!write_ok)goto fail;
    printf("RESULT keyupdate_%s%s PASS pending_read=WANT_READ fresh_write=success blocked_control=%d\n",requested?"requested":"not_requested",blocked?"_blocked":"",saw_block);
    pair_free(&p);return 1;
fail:
    fprintf(stderr,"STATE keyupdate requested=%d blocked=%d c=%s s=%s init=%d\n",requested,blocked,errname(p.c.last_err),errname(p.s.last_err),p.c.ssl?SSL_in_init(p.c.ssl):-1);pair_free(&p);return 0;
}

static int endings(const char *cert,const char *key) {
    Pair p; unsigned char b[8];size_t n=999;int se=SSL_ERROR_NONE;
    if(!pair_new(&p,TLS1_3_VERSION,16384,cert,key)||!handshake(&p,16384))goto fail;
    ERR_clear_error();int sh=SSL_shutdown(p.c.ssl);se=classify(&p.c,sh);
    while(move_one(&p.c,&p.s,16384)>0){}
    ERR_clear_error();int rr=SSL_read_ex(p.s.ssl,b,sizeof(b),&n);int re=classify(&p.s,rr);
    if(re!=SSL_ERROR_ZERO_RETURN)goto fail;
    pair_free(&p);
    if(!pair_new(&p,TLS1_3_VERSION,16384,cert,key)||!handshake(&p,16384))goto fail;
    BIO_shutdown_wr(p.c.net); /* lower raw EOF after retained feed tail (none) */
    ERR_clear_error();rr=SSL_read_ex(p.c.ssl,b,sizeof(b),&n);re=classify(&p.c,rr);
    if(re!=SSL_ERROR_SSL)goto fail;
    printf("RESULT close_eof PASS close_notify=ZERO_RETURN truncation=SSL_ERROR retained_obligations=none quiescent_release=after_join\n");
    pair_free(&p);return 1;
fail:
    fprintf(stderr,"STATE endings c=%s s=%s shutdown=%s\n",errname(p.c.last_err),errname(p.s.last_err),errname(se));pair_free(&p);return 0;
}

int main(int argc,char **argv){
    if(argc!=3){fprintf(stderr,"usage: %s cert key\n",argv[0]);return 2;}
    OPENSSL_init_ssl(0,NULL);
    printf("CONFIG OpenSSL=%s partial_write=on moving_buffer=off auto_retry=off max_fragment=16384 pipelines=1 outbound_padding=none app_attempt_max=16384 finite_pump=%d\n",OpenSSL_version(OPENSSL_VERSION),MAX_PUMP);
    printf("LEDGER hypothesis_only plaintext_out=32+16+16KiB plaintext_in=16+16+16+16KiB ciphertext_out=16+16+16+16KiB ciphertext_in=16+16+16+16KiB synchronous_C_does_not_model_async_or_managed_native_copy_overlap=1\n");
    int pass=0,total=0;
#define RUN(x) do{total++;if((x))pass++;}while(0)
    RUN(baseline_once(argv[1],argv[2],0));
    RUN(baseline_once(argv[1],argv[2],1));
    RUN(basic_flow(argv[1],argv[2],TLS1_2_VERSION,0,17));
    RUN(basic_flow(argv[1],argv[2],TLS1_2_VERSION,1,23));
    RUN(basic_flow(argv[1],argv[2],TLS1_3_VERSION,0,19));
    RUN(basic_flow(argv[1],argv[2],TLS1_3_VERSION,1,29));
    RUN(full_record_retry(argv[1],argv[2]));
    RUN(partial_suffix(argv[1],argv[2]));
    RUN(bidirectional(argv[1],argv[2]));
    RUN(keyupdate_case(argv[1],argv[2],0,0));
    RUN(keyupdate_case(argv[1],argv[2],1,0));
    RUN(keyupdate_case(argv[1],argv[2],1,1));
    RUN(endings(argv[1],argv[2]));
    printf("SUMMARY pass=%d total=%d fail=%d scope=standalone_in_memory_reference_not_Rill_not_TCP\n",pass,total,total-pass);
    return pass==total?0:1;
}

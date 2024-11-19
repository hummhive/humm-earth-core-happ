<template>
  <div v-if="loading" style="display: flex; flex: 1; align-items: center; justify-content: center">
    <mwc-circular-progress indeterminate></mwc-circular-progress>
  </div>

  <div v-else style="display: flex; flex-direction: column">
    <span v-if="error">Error fetching the encrypted contents: {{error.data.data}}.</span>
    <div v-else-if="hashes && hashes.length > 0" style="margin-bottom: 8px">
      <EncryptedContentDetail 
        v-for="hash in hashes" 
        :encrypted-content-hash="hash"
        @encrypted-content-deleted="fetchEncryptedContent()"
      >
      </EncryptedContentDetail>
    </div>
    <span v-else>No encrypted contents found for this author.</span>
  </div>

</template>

<script lang="ts">
import { defineComponent, inject, toRaw, ComputedRef } from 'vue';
import { decode } from '@msgpack/msgpack';
import { AppClient, NewEntryAction, Record, AgentPubKey, EntryHash, ActionHash } from '@holochain/client';
import '@material/mwc-circular-progress';
import EncryptedContentDetail from './EncryptedContentDetail.vue';
import { ContentSignal } from './types';

export default defineComponent({
  components: {
    EncryptedContentDetail
  },
  props: {
    author: {
      type: Object,
      required: true
    }
  },
  data(): { hashes: Array<ActionHash> | undefined; loading: boolean; error: any } {
    return {
      hashes: undefined,
      loading: true,
      error: undefined
    }
  },
  async mounted() {
    if (this.author === undefined) {
      throw new Error(`The author property is required for the AllEncryptedContent element`);
    }

    await this.fetchEncryptedContent();
    toRaw(this.client).on('signal', signal => {
      if (signal.zome_name !== 'content') return; 
      const payload = signal.payload as ContentSignal;
      if (payload.type !== 'EntryCreated') return;
      if (payload.app_entry.type !== 'EncryptedContent') return;
      if (this.author.toString() !== this.client.myPubKey.toString()) return;
      if (this.hashes) this.hashes.push(payload.action.hashed.hash);
    });
  },
  methods: {
    async fetchEncryptedContent() {
      try {
        const records: Array<Record> = await this.client.callZome({
          cap_secret: null,
          role_name: 'humm_earth_core',
          zome_name: 'content',
          fn_name: 'get_all_encrypted_content_by_author',
          payload: this.author,
        });
        this.hashes = records.map(r => r.signed_action.hashed.hash);
      } catch (e) {
        this.error = e;
      }
      this.loading = false;
    }
  },
  setup() {
    const client = (inject('client') as ComputedRef<AppClient>).value;
    return {
      client,
    };
  },
})
</script>

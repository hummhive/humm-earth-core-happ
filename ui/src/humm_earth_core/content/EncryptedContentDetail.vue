<template>
  <div v-if="!loading">
    <div v-if="editing" style="display: flex; flex: 1;">
      <EditEncryptedContent
        :original-encrypted-content-hash="encryptedContentHash"
        :current-record="record!"
        @encrypted-content-updated="editing = false; fetchEncryptedContent();"
        @edit-canceled="editing = false"
      ></EditEncryptedContent>
    </div>
    <div v-else-if="record" style="display: flex; flex-direction: column">
      <div style="display: flex; flex-direction: row">
        <span style="flex: 1"></span>
      
        <mwc-icon-button style="margin-left: 8px" icon="edit" @click="editing = true"></mwc-icon-button>
        <mwc-icon-button style="margin-left: 8px" icon="delete" @click="deleteEncryptedContent()"></mwc-icon-button>
      </div>

    </div>
    
    <span v-else>The requested encrypted content was not found.</span>
  </div>

  <div v-else style="display: flex; flex: 1; align-items: center; justify-content: center">
    <mwc-circular-progress indeterminate></mwc-circular-progress>
  </div>

  <mwc-snackbar ref="delete-error" leading>
  </mwc-snackbar>
</template>

<script lang="ts">
import { defineComponent, inject, ComputedRef } from 'vue';
import { decode } from '@msgpack/msgpack';
import { AppClient, Record, AgentPubKey, EntryHash, ActionHash, DnaHash } from '@holochain/client';
import { EncryptedContent } from './types';
import '@material/mwc-circular-progress';
import '@material/mwc-icon-button';
import '@material/mwc-snackbar';
import { Snackbar } from '@material/mwc-snackbar';
import EditEncryptedContent from './EditEncryptedContent.vue';

export default defineComponent({
  components: {
    EditEncryptedContent
  },
  props: {
    encryptedContentHash: {
      type: Object,
      required: true
    }
  },
  data(): { record: Record | undefined; loading: boolean; editing: boolean; } {
    return {
      record: undefined,
      loading: true,
      editing: false,
    }
  },
  computed: {
    encryptedContent() {
      if (!this.record) return undefined;
      return decode((this.record.entry as any).Present.entry) as EncryptedContent;
    }
  },
  async mounted() {
    if (this.encryptedContentHash === undefined) {
      throw new Error(`The encryptedContentHash input is required for the EncryptedContentDetail element`);
    }

    await this.fetchEncryptedContent();
  },
  methods: {
    async fetchEncryptedContent() {
      this.loading = true;
      this.record = undefined;

      this.record = await this.client.callZome({
        cap_secret: null,
        role_name: 'humm_earth_core',
        zome_name: 'content',
        fn_name: 'get_encrypted_content',
        payload: this.encryptedContentHash,
      });

      this.loading = false;
    },
    async deleteEncryptedContent() {
      try {
        await this.client.callZome({
          cap_secret: null,
          role_name: 'humm_earth_core',
          zome_name: 'content',
          fn_name: 'delete_encrypted_content',
          payload: this.encryptedContentHash,
        });
        this.$emit('encrypted-content-deleted', this.encryptedContentHash);
        this.fetchEncryptedContent();
      } catch (e: any) {
        const errorSnackbar = this.$refs['delete-error'] as Snackbar;
        errorSnackbar.labelText = `Error deleting the encrypted content: ${e.data.data}`;
        errorSnackbar.show();
      }
    }
  },
  emits: ['encrypted-content-deleted'],
  setup() {
    const client = (inject('client') as ComputedRef<AppClient>).value;
    return {
      client
    };
  },
})
</script>

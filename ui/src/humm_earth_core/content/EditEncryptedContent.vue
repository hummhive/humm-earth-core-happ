<template>
  <mwc-snackbar ref="update-error"></mwc-snackbar>

  <div style="display: flex; flex-direction: column">
    <span style="font-size: 18px">Edit Encrypted Content</span>


    <div style="display: flex; flex-direction: row">
      <mwc-button
        outlined
        label="Cancel"
        @click="$emit('edit-canceled')"
        style="flex: 1; margin-right: 16px;"
      ></mwc-button>
      <mwc-button 
        raised
        label="Save"
        :disabled="!isEncryptedContentValid"
        @click="updateEncryptedContent"
        style="flex: 1;"
      ></mwc-button>
    </div>
  </div>
</template>
<script lang="ts">
import { defineComponent, inject, ComputedRef } from 'vue';
import { AppClient, Record, AgentPubKey, EntryHash, ActionHash, DnaHash } from '@holochain/client';
import { EncryptedContent } from './types';
import '@material/mwc-button';
import '@material/mwc-snackbar';
import { decode } from '@msgpack/msgpack';
import { Snackbar } from '@material/mwc-snackbar';

export default defineComponent({
  data(): {
  } {
    const currentEncryptedContent = decode((this.currentRecord.entry as any).Present.entry) as EncryptedContent;
    return { 
    }
  },
  props: {
    originalEncryptedContentHash: {
      type: null,
      required: true,
    },
    currentRecord: {
      type: Object,
      required: true
    }
  },
  computed: {
    currentEncryptedContent() {
      return decode((this.currentRecord.entry as any).Present.entry) as EncryptedContent;
    },
    isEncryptedContentValid() {
      return true;
    },
  },
  mounted() {
    if (this.currentRecord === undefined) {
      throw new Error(`The currentRecord input is required for the EditEncryptedContent element`);
    }
    if (this.originalEncryptedContentHash === undefined) {
      throw new Error(`The originalEncryptedContentHash input is required for the EditEncryptedContent element`);
    }
  },
  methods: {
    async updateEncryptedContent() {

      const encryptedContent: EncryptedContent = { 
        id: this.currentEncryptedContent.id,
        content_type: this.currentEncryptedContent.content_type,
        bytes: this.currentEncryptedContent.bytes,
      };

      try {
        const updateRecord: Record = await this.client.callZome({
          cap_secret: null,
          role_name: 'humm_earth_core',
          zome_name: 'content',
          fn_name: 'update_encrypted_content',
          payload: {
            original_encrypted_content_hash: this.originalEncryptedContentHash,
            previous_encrypted_content_hash: this.currentRecord.signed_action.hashed.hash,
            updated_encrypted_content: encryptedContent
          }
        });
        this.$emit('encrypted-content-updated', updateRecord.signed_action.hashed.hash);
      } catch (e: any) {
        const errorSnackbar = this.$refs['update-error'] as Snackbar;
        errorSnackbar.labelText = `Error updating the encrypted content: ${e.data.data}`;
        errorSnackbar.show();
      }
    },
  },
  emits: ['encrypted-content-updated', 'edit-canceled'],
  setup() {
    const client = (inject('client') as ComputedRef<AppClient>).value;
    return {
      client,
    };
  },
})
</script>

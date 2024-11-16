//Karter Arritt
//karrittq/referral_list_endpoint

/** Instructions
 * Set up a Google Sheet
 * Extensions -> Apps Script -> Copy/Paste this file
 * 
 * Google Sheet Requirements: 
 *   -have a "out" tab (you can hide this if you want)
 *   -have a "config" tab with cell B2 holding your decryption key - the same one you provide when running the referral_list_endpoint.exe
 *   -have a "formatted" tab
 * 
 * 
 * To set up your referral_list_endpoint.exe file, you will need the POSTurl. Once you've set up this file in Google Apps Script: 
 *   -Click Deploy -> New Deployment.
 *   -Type: Webapp
 *   -Execute As: Me
 *   -Who can access: anyone
 *   -give it whatever description you want.
 * after that, copy the webapp url and paste it into the exe file or into your .env file.
 * 
 * You can then use the results as you want.
 * 
**/


const sheet = SpreadsheetApp.getActiveSpreadsheet();
const CRYPT_KEY = sheet.getSheetByName("config").getRange("B2").getValue();


let printIndex = 1;
/**
 * prints a message to the out tab, as there isn't a console
 * 
 * @param {String} message - the message you want to log
 */
function print(message) {
  splitStringIntoChunks(message).forEach((chunk) => {
    try {
      Logger.log('Writing message to row: ' + printIndex);
      sheet.getSheetByName("out").getRange("A" + printIndex).setValue(chunk); // Writing chunk to cell
    } catch (e) {
      console.warn("Warning: " + e);
    }
    printIndex++; // Increment row index for each chunk
  });
  printIndex++;

  function splitStringIntoChunks(inputString, maxLength = 40000) {
    const chunks = [];
    
    // Split the input string into chunks of 'maxLength' or less
    for (let i = 0; i < inputString.length; i += maxLength) {
      chunks.push(inputString.slice(i, i + maxLength));
    }

    return chunks;
  }
}

// Base64 decoding function (without external libraries)
function base64Decode(base64) {
    const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let str = "";
    let buffer = 0;
    let bits = 0;

    // Remove padding
    base64 = base64.replace(/=/g, "");

    for (let i = 0; i < base64.length; i++) {
        let currentChar = base64.charAt(i);
        let index = chars.indexOf(currentChar);
        if (index === -1) continue;

        buffer = (buffer << 6) | index;
        bits += 6;

        if (bits >= 8) {
            bits -= 8;
            str += String.fromCharCode((buffer >> bits) & 0xFF);
            buffer &= (1 << bits) - 1;
        }
    }

    return str;
}

// Decrypt function using encryption key
function decryptWithOTP(encryptedBase64, cryptKey) {
    // Step 1: Base64 decode the encrypted string
    print("Encoded String: "+ encryptedBase64);

    const encryptedString = atob(encryptedBase64).trim(); // Use `atob` instead of `base64Decode`
    print("Encrypted String: "+encryptedString);

    // Step 2: Decrypt the string by XORing each byte with cryptKey byte
    let decryptedString = "";
    for (let i = 0; i < encryptedString.length; i++) {
        const encryptedByte = encryptedString.charCodeAt(i);

        // Use modulo to loop through the cryptKey if it's shorter than the encrypted string
        const cryptByte = cryptKey.charCodeAt(i % cryptKey.length);

        // XOR the encrypted byte with the cryptKey byte to decrypt
        const decryptedByte = encryptedByte ^ cryptByte;
        decryptedString += String.fromCharCode(decryptedByte);
    }
    
    print("Decrypted String: "+ decryptedString);

    // Step 3: Parse the decrypted JSON string into a JavaScript object
    try{return JSON.parse(decryptedString)} catch (e) {print("JSON Parse Failed: "+ e); print(decryptedString); return e;};
}

function atob(base64) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
    let output = '';
    let buffer = 0;
    let padding = 0;

    // Remove any base64 padding '=' characters
    base64 = base64.replace(/[^A-Za-z0-9+/]/g, '');

    // Loop through each base64 character and decode
    for (let i = 0; i < base64.length; i++) {
        const char = base64.charAt(i);
        const charIndex = chars.indexOf(char);

        // If an invalid character is encountered, throw an error
        if (charIndex === -1) {
            throw new Error("Invalid base64 character encountered.");
        }

        // Add the 6 bits of the current character to the buffer
        buffer = (buffer << 6) | charIndex;

        // Every 4 base64 characters (24 bits) form 3 original bytes
        if ((i + 1) % 4 === 0 || i === base64.length - 1) {
            // Extract the 8-bit chunks (bytes)
            for (let j = 16; j >= 0; j -= 8) {
                output += String.fromCharCode((buffer >> j) & 0xFF);
            }

            // Reset buffer for the next group of 4 characters
            buffer = 0;
        }
    }

    return output;
}

// Google Apps Script doPost function to handle the POST request
function doPost(e) {
    sheet.getSheetByName("out").clear();

    // Parse the incoming JSON payload
    try{
      const data = getDataOut(JSON.parse(e.postData.contents));
    
      // Convert the data to a 2D array
      const pivotedData = convertTo2DArray(data);
      
      // Access the Google Sheet named "formatted"
      const formatSheet = sheet.getSheetByName("formatted");

      // Clear any existing content in the sheet
      formatSheet.getRange("C1:Z").clear();
      
      // Set the values starting from cell A1
      formatSheet.getRange(3, 3, pivotedData.length, pivotedData[0].length).setValues(pivotedData);
      formatSheet.getRange("C1").setValue(new Date());
    } catch (err) {
      print("Error: "+err);
    }
}

function convertTo2DArray(arr) {
  try{if (arr.length === 0) return []; // Return empty array if input is empty

  // Extract the keys from the first object in the array
  const keys = Object.keys(arr[0]);

  // Map over the array of objects and create rows with values for each key
  const values = arr.map(obj => keys.map(key => obj[key]));

  // Return the 2D array, with keys as the first row followed by the values
  return [keys, ...values];} catch{ return [["rip"]];}
}

function getDataOut(data){
  const encryptedBase64 = data.body;  // The encrypted Base64 string
  //print(data.body);
  //print(data.body);

  try {return JSON.parse(encryptedBase64);} catch {} //in case it was sent unencrypted -- it shouldn't have been though

  // Decrypt the data with OTP
  const decryptedObject = decryptWithOTP(encryptedBase64, CRYPT_KEY);
  
  // Return the decrypted object as a JSON response
  try{print(JSON.stringify(decryptedObject));} catch {
    decryptedObject += "]";
    print(JSON.stringify(decryptedObject));
  }
  return decryptedObject;
}
